#!/bin/sh
# REPORT installer — run on a fresh Pi OS Lite 64-bit with internet access.
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
    libcairo2 \
    v4l-utils \
    avahi-utils \
    libdrm-tests

sudo mkdir -p /etc/systemd/journald.conf.d
sudo cp "$(dirname "$0")/journald-precrime.conf" /etc/systemd/journald.conf.d/precrime.conf
sudo systemctl restart systemd-journald
echo "journald log rotation configured (200M max)"

echo "Install complete. Run 'make deploy-report REPORT_HOST=$(hostname)' from the macOS dev machine."
