#!/bin/sh
# PRECOG Kit A installer — run on a fresh Pi OS Lite 64-bit with internet access
set -e

echo "Installing GStreamer + Rust plugins + build deps + v4l-utils..."
sudo apt update
sudo apt install -y \
    build-essential pkg-config curl \
    libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev \
    gstreamer1.0-tools \
    gstreamer1.0-plugins-base \
    gstreamer1.0-plugins-good \
    gstreamer1.0-plugins-bad \
    gstreamer1.0-plugins-ugly \
    gstreamer1.0-plugins-rs \
    libudev-dev \
    v4l-utils

if ! command -v rustc >/dev/null 2>&1; then
    echo "Installing rustup + stable Rust..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
fi

echo ""
echo "Next: manually install the NDI SDK runtime libndi.so per the runbook."
echo "Then run: gst-inspect-1.0 ndisink"
echo "If it succeeds, install is complete."
