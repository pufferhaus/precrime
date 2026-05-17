import Foundation
import Darwin
import os.log

/// UDP socket bound to the WiFi interface, sending RTP packets to a configured
/// unicast host:port. Bound to en0 explicitly because iOS doesn't auto-pick an
/// outgoing interface for non-route-default destinations (proven by the earlier
/// MulticastProbe — same path here for unicast keeps behavior identical when
/// we later swap to multicast on entitlement approval).
final class RtpSender {
    private static let logger = Logger(subsystem: "art.precrime.PrecogCam", category: "RtpSender")
    private var fd: Int32 = -1
    private var targetAddr = sockaddr_in()
    private(set) var host: String
    private(set) var port: UInt16
    private(set) var packetsSent: UInt64 = 0
    private(set) var bytesSent: UInt64 = 0

    /// False until `retarget(host:port:)` succeeds at least once.
    /// `sendBatch` / `send` are no-ops while this is false.
    private(set) var targeted: Bool

    init?(host: String, port: UInt16) {
        self.host = host
        self.port = port
        self.targeted = false
        guard configure(host: host, port: port) else { return nil }
        targeted = true
    }

    /// Create a sender with no initial target. Packets are dropped until
    /// `retarget(host:port:)` is called successfully.
    init() {
        self.host = ""
        self.port = 0
        self.targeted = false
    }

    deinit {
        if fd >= 0 { close(fd) }
    }

    /// Reconfigure for a new destination. Existing fd is closed.
    @discardableResult
    func retarget(host: String, port: UInt16) -> Bool {
        if fd >= 0 { close(fd); fd = -1 }
        let ok = configure(host: host, port: port)
        if ok { targeted = true }
        return ok
    }

    /// Drop the current target. `send`/`sendBatch` become no-ops again.
    func untarget() {
        if fd >= 0 { close(fd); fd = -1 }
        targeted = false
    }

    private func configure(host: String, port: UInt16) -> Bool {
        let s = socket(AF_INET, SOCK_DGRAM, IPPROTO_UDP)
        guard s >= 0 else {
            Self.logger.error("socket() failed errno=\(errno)")
            return false
        }

        // Bind socket egress to en0 (WiFi). IP_BOUND_IF for unicast routing.
        if let idx = wifiInterfaceIndex() {
            var i = idx
            _ = setsockopt(s, IPPROTO_IP, IP_BOUND_IF, &i, socklen_t(MemoryLayout<UInt32>.size))
        }

        // Larger send buffer reduces drops at high bitrate.
        var sndBuf: Int32 = 2 * 1024 * 1024
        _ = setsockopt(s, SOL_SOCKET, SO_SNDBUF, &sndBuf, socklen_t(MemoryLayout<Int32>.size))

        var addr = sockaddr_in()
        addr.sin_family = sa_family_t(AF_INET)
        addr.sin_port = port.bigEndian
        guard inet_pton(AF_INET, host, &addr.sin_addr) == 1 else {
            Self.logger.error("inet_pton failed for host '\(host, privacy: .public)'")
            close(s)
            return false
        }

        self.fd = s
        self.targetAddr = addr
        self.host = host
        self.port = port
        self.packetsSent = 0
        self.bytesSent = 0
        Self.logger.info("RTP sender → \(host, privacy: .public):\(port)")
        return true
    }

    /// Send one already-built RTP packet. Drops silently on transient errors;
    /// logs once per error type. No-op until `retarget()` has been called.
    func send(_ packet: Data) {
        guard fd >= 0, targeted else { return }
        let n = packet.withUnsafeBytes { buf -> Int in
            withUnsafePointer(to: &targetAddr) { addrPtr -> Int in
                addrPtr.withMemoryRebound(to: sockaddr.self, capacity: 1) { sa in
                    sendto(fd, buf.baseAddress, buf.count, 0, sa, socklen_t(MemoryLayout<sockaddr_in>.size))
                }
            }
        }
        if n >= 0 {
            packetsSent &+= 1
            bytesSent &+= UInt64(n)
        } else {
            let e = errno
            Self.logger.error("sendto errno=\(e) (\(String(cString: strerror(e))))")
        }
    }

    func sendBatch(_ packets: [Data]) {
        for p in packets { send(p) }
    }
}

/// Walk getifaddrs() for an active IPv4 interface named en0 (WiFi on iPhone).
private func wifiInterfaceIndex() -> UInt32? {
    var ifaddrPtr: UnsafeMutablePointer<ifaddrs>?
    guard getifaddrs(&ifaddrPtr) == 0, let first = ifaddrPtr else { return nil }
    defer { freeifaddrs(ifaddrPtr) }

    var cur: UnsafeMutablePointer<ifaddrs>? = first
    while let p = cur {
        let ifa = p.pointee
        let flags = Int32(ifa.ifa_flags)
        let isUp = (flags & IFF_UP) != 0
        let isRunning = (flags & IFF_RUNNING) != 0
        let isLoop = (flags & IFF_LOOPBACK) != 0
        if let addr = ifa.ifa_addr,
           addr.pointee.sa_family == sa_family_t(AF_INET),
           isUp, isRunning, !isLoop,
           let name = ifa.ifa_name.map({ String(cString: $0) }),
           name == "en0" {
            let idx = if_nametoindex(name)
            return idx == 0 ? nil : idx
        }
        cur = ifa.ifa_next
    }
    return nil
}
