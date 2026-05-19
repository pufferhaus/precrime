#!/bin/sh
# PRECOG Kit A installer — run on a fresh Pi OS Lite 64-bit with internet access
# Binaries are built on macOS via cross and deployed via rsync.
set -e

sudo apt update
sudo apt install -y \
    gstreamer1.0-tools \
    gstreamer1.0-plugins-base \
    gstreamer1.0-plugins-good \
    gstreamer1.0-plugins-bad \
    gstreamer1.0-plugins-ugly \
    gstreamer1.0-libav \
    v4l-utils

echo "Install complete. Run 'make deploy-precog PRECOG_HOST=$(hostname)' from the macOS dev machine."
