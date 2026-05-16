# PRECOG Kit A — CCTV Encoder Runbook

## Identity
- Hostname: `precog-02-cctv-door`
- NDI display name: `PRECOG-02-CCTV-DOOR`
- Hardware: Pi 5 4GB + active cooler + EasyCap UTV007 + vintage CCTV cam

## Setup history
- 2026-05-16: Rust crate created per precog-kit-a-cctv plan (SUB-4)

## Hardware setup (filled in when Pi arrives)

See `docs/plans/2026-05-16-precog-kit-a-cctv.md` Tasks 1–5 for
Pi flash, dependency install, EasyCap probe, and M3/M4 verification steps.

## M4 verification

To be filled in when CCTV cam is connected and producing live NDI feed.

## Boot-survival verification

To be filled in after `precog.service` is installed and a reboot cycle is
verified.

## Show-day pre-flight checklist

- [ ] CCTV cam powered up, video signal generating
- [ ] BNC → RCA adapter seated, RCA in EasyCap yellow jack
- [ ] EasyCap plugged into Pi 5 USB
- [ ] Pi 5 plugged into USB-C PD power (wall or 10000mAh PD bank)
- [ ] Pi 5 boots within ~30s
- [ ] On operator laptop / REPORT multiview: confirm `PRECOG-02-CCTV-DOOR` is live

## Tear-down

- [ ] Power down Pi: `ssh cody@precog-02-cctv-door.local 'sudo shutdown -h now'`, wait for green LED to stop
- [ ] Disconnect EasyCap and CCTV cam, coil cables
- [ ] Pack into kit case

## Troubleshooting

- **NDI source not appearing:** `sudo systemctl status precog.service`; if failed, `sudo journalctl -u precog.service -n 50 --no-pager`.
- **NDI source appears but no video:** `v4l2-ctl -d /dev/video0 --stream-mmap=3 --stream-count=1 --stream-to=/tmp/frame.raw` — inspect file size; near-zero implies cam power or BNC cable problem.
- **Harsh interlace / color shift:** correct, that is the CCTV aesthetic.
