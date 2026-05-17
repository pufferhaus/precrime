# Local RTP+Multicast Smoke Test (macOS)

Verifies precog → RTP/multicast → standalone gst-launch receiver without a Pi.

## Prereqs

- gstreamer + plugins-good/bad/ugly + libav installed via Homebrew:
  ```bash
  brew install gstreamer gst-plugins-good gst-plugins-bad gst-plugins-ugly gst-libav
  ```
- `cargo build --workspace` succeeds.
- Loopback interface allows multicast (one-time per boot if missing):
  ```bash
  sudo route -n add -net 239 -interface lo0
  ```

## Procedure

### Terminal A — precog publishes the built-in webcam as PRECOG-99-MAC-TEST

```bash
cat > /tmp/precog-mac.toml <<'EOF'
source_name = "PRECOG-99-MAC-TEST"
device = "0"
format = "UYVY"
width = 1280
height = 720
framerate = "30/1"
rtp_mcast = "239.42.1.99"
rtp_port = 5000
EOF
PRECOG_CONFIG=/tmp/precog-mac.toml RUST_LOG=info cargo run -p precog
```

Expect: log line `PRECOG starting`; webcam LED on.

### Terminal B — gst-launch standalone RTP receiver

```bash
gst-launch-1.0 -v \
  udpsrc address=239.42.1.99 port=5000 auto-multicast=true \
  caps="application/x-rtp,media=video,clock-rate=90000,encoding-name=H264,payload=96" ! \
  rtpjitterbuffer latency=20 ! \
  rtph264depay ! h264parse ! avdec_h264 ! \
  videoconvert ! osxvideosink
```

Expect: webcam frames appear within ~3 seconds of `PLAYING`.

### Terminal C — verify ball is being emitted

```bash
gst-launch-1.0 -v \
  udpsrc address=239.42.0.1 port=9999 auto-multicast=true ! \
  fakesink dump=true
```

Expect: hex dump containing `"name":"PRECOG-99-MAC-TEST"` every ~2 seconds.

## Failure modes

- **udpsink "Could not get/set settings from/on resource":** macOS multicast route missing — see prereqs.
- **No frames in B but precog log clean:** check `tcpdump -i lo0 -nn udp port 5000`. Zero packets = route issue; packets present but no frames = caps mismatch (verify width/height/framerate).
- **Ball visible (C) but no RTP (B):** per-source `rtp_mcast`/`rtp_port` mismatch between TOML and the udpsrc line.
- **Precog exits with "rtp_mcast … is not a multicast address":** validation guard fired. Fix the TOML; multicast range is 224.0.0.0/4 (typically 239.x.x.x for admin-scoped).

## Architectural gate

If local loopback doesn't render video on the mac, no Pi deploy will. Stop here and debug before pushing the workspace branch to Pi hardware.
