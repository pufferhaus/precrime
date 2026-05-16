#!/bin/sh
set -e
echo "Installing GStreamer, build deps, and Rust toolchain..."
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
    libcairo2-dev \
    libudev-dev \
    v4l-utils

if ! command -v rustc >/dev/null 2>&1; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
fi

echo ""
echo "Next: manually install NDI SDK runtime libndi.so per the runbook."
echo "Then run: gst-inspect-1.0 ndisrc"
