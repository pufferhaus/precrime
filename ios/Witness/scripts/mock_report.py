#!/usr/bin/env python3
"""
mock_report.py — PRECRIME REPORT mock for macOS dev testing.

Implements the full WITNESS discovery/registration/ack protocol:
  1. Publishes _precrime-report._tcp via dns-sd (Bonjour)
  2. TCP :4999 — accepts WITNESS registrations, assigns RTP ports
  3. UDP acks → registered sources every 2s
  4. UDP :500x — shows RTP packet rate per source

Usage:
  python3 scripts/mock_report.py
  python3 scripts/mock_report.py --name REPORT-STAGE --reg-port 4999
"""

import argparse
import json
import os
import signal
import socket
import subprocess
import sys
import threading
import time
from dataclasses import dataclass, field
from typing import Dict, Optional

ACK_PORT = 9998
RTP_PORT_BASE = 5000
RTP_PORT_MAX = 5099

@dataclass
class RegisteredSource:
    name: str
    host_ip: str
    assigned_port: int
    registered_at: float = field(default_factory=time.time)
    last_seen: float = field(default_factory=time.time)
    packets_received: int = 0

class MockReport:
    def __init__(self, name: str, reg_port: int):
        self.name = name
        self.reg_port = reg_port
        self.sources: Dict[str, RegisteredSource] = {}
        self.lock = threading.Lock()
        self.port_pool = list(range(RTP_PORT_BASE, RTP_PORT_MAX + 1))
        self.shutdown = threading.Event()
        self._bonjour_proc: Optional[subprocess.Popen] = None
        self._ack_sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)

    def start(self):
        self._publish_bonjour()
        threading.Thread(target=self._tcp_server, daemon=True).start()
        threading.Thread(target=self._ack_loop, daemon=True).start()
        print(f"\n[REPORT] {self.name} online")
        print(f"  Bonjour: _precrime-report._tcp (reg_port={self.reg_port})")
        print(f"  TCP registration: :{self.reg_port}")
        print(f"  UDP ack: → source:{ACK_PORT} every 2s")
        print(f"  RTP pool: {RTP_PORT_BASE}–{RTP_PORT_MAX}\n")

    def stop(self):
        self.shutdown.set()
        if self._bonjour_proc:
            self._bonjour_proc.terminate()
        print("\n[REPORT] shutdown")

    # ── Bonjour ──────────────────────────────────────────────────────────────

    def _publish_bonjour(self):
        try:
            self._bonjour_proc = subprocess.Popen(
                ["dns-sd", "-R", self.name, "_precrime-report._tcp", ".",
                 str(self.reg_port), f"v=1", f"reg_port={self.reg_port}"],
                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL
            )
            print(f"[bonjour] published {self.name}._precrime-report._tcp port {self.reg_port}")
        except FileNotFoundError:
            print("[bonjour] dns-sd not found — Bonjour publish skipped")

    # ── TCP registration server ───────────────────────────────────────────────

    def _tcp_server(self):
        srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        srv.bind(("0.0.0.0", self.reg_port))
        srv.listen(16)
        srv.settimeout(1.0)
        while not self.shutdown.is_set():
            try:
                conn, addr = srv.accept()
                threading.Thread(
                    target=self._handle_registration,
                    args=(conn, addr[0]),
                    daemon=True
                ).start()
            except socket.timeout:
                continue
        srv.close()

    def _handle_registration(self, conn: socket.socket, client_ip: str):
        try:
            conn.settimeout(5.0)
            data = b""
            while b"\n" not in data:
                chunk = conn.recv(1024)
                if not chunk:
                    return
                data += chunk
            req = json.loads(data.split(b"\n")[0])
            name = req.get("name", "UNKNOWN")

            with self.lock:
                if name in self.sources:
                    # Re-registration (keep-alive) — reuse port
                    src = self.sources[name]
                    src.last_seen = time.time()
                    port = src.assigned_port
                    action = "re-registered"
                else:
                    if not self.port_pool:
                        print(f"[reg] ERROR: port pool exhausted")
                        return
                    port = self.port_pool.pop(0)
                    self.sources[name] = RegisteredSource(
                        name=name, host_ip=client_ip, assigned_port=port
                    )
                    action = "registered"
                    threading.Thread(
                        target=self._rtp_listener, args=(name, port), daemon=True
                    ).start()

            resp = json.dumps({
                "assigned_port": port,
                "report_name": self.name,
                "ack_port": ACK_PORT
            })
            conn.sendall((resp + "\n").encode())
            res = req.get("resolution", "?")
            fps = req.get("fps", "?")
            kbps = req.get("bitrate_kbps", "?")
            print(f"[reg] {action}: {name!r} @ {client_ip} → port {port}  ({res} {fps}fps {kbps}kbps)")
        except Exception as e:
            print(f"[reg] error from {client_ip}: {e}")
        finally:
            conn.close()

    # ── Ack sender ────────────────────────────────────────────────────────────

    def _ack_loop(self):
        while not self.shutdown.is_set():
            time.sleep(2)
            payload = json.dumps({"v": "1", "report": self.name, "ts": int(time.time())}).encode()
            with self.lock:
                srcs = list(self.sources.values())
            for src in srcs:
                try:
                    self._ack_sock.sendto(payload, (src.host_ip, ACK_PORT))
                except Exception:
                    pass

    # ── RTP listener (shows packet rate) ─────────────────────────────────────

    def _rtp_listener(self, name: str, port: int):
        sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        sock.bind(("0.0.0.0", port))
        sock.settimeout(1.0)
        # Tee socket — forwards every RTP packet to localhost:(port+200) for gst-launch viewing
        tee = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        view_port = port + 200
        last_report = time.time()
        packets = 0
        print(f"[rtp] listening on :{port} for {name!r}  (view → :{view_port})")
        while not self.shutdown.is_set():
            try:
                data, _ = sock.recvfrom(2048)
                packets += 1
                tee.sendto(data, ("127.0.0.1", view_port))
                with self.lock:
                    if name in self.sources:
                        self.sources[name].packets_received += 1
                        self.sources[name].last_seen = time.time()
            except socket.timeout:
                pass
            now = time.time()
            if now - last_report >= 5:
                pps = packets / (now - last_report)
                kbps_approx = pps * 1400 * 8 / 1000
                print(f"[rtp] {name!r} port {port}: {pps:.1f} pkt/s ≈ {kbps_approx:.0f} kbps")
                packets = 0
                last_report = now
        sock.close()
        tee.close()

    # ── Status loop ───────────────────────────────────────────────────────────

    def status_loop(self):
        try:
            while not self.shutdown.is_set():
                time.sleep(10)
                with self.lock:
                    if self.sources:
                        print(f"\n[status] {len(self.sources)} source(s):")
                        for s in self.sources.values():
                            age = time.time() - s.last_seen
                            print(f"  {s.name!r:30s} {s.host_ip}:{s.assigned_port}  "
                                  f"pkts={s.packets_received}  last_seen={age:.0f}s ago")
                    else:
                        print("[status] no sources registered yet")
        except KeyboardInterrupt:
            pass


def main():
    p = argparse.ArgumentParser(description="PRECRIME REPORT mock server")
    p.add_argument("--name", default="REPORT-MAIN")
    p.add_argument("--reg-port", type=int, default=4999)
    args = p.parse_args()

    report = MockReport(name=args.name, reg_port=args.reg_port)
    report.start()

    def handle_signal(sig, frame):
        report.stop()
        sys.exit(0)

    signal.signal(signal.SIGINT, handle_signal)
    signal.signal(signal.SIGTERM, handle_signal)
    report.status_loop()


if __name__ == "__main__":
    main()
