#!/usr/bin/env bash
# Receive H.264 RTP from PrecogCam and display. Pass --save to grab JPEGs instead.
set -e
PORT="${PORT:-5000}"
CAPS='application/x-rtp,media=video,clock-rate=90000,encoding-name=H264,payload=96'
if [ "$1" = "--save" ]; then
  exec gst-launch-1.0 -e \
    udpsrc port=$PORT caps="$CAPS" ! \
    rtpjitterbuffer latency=80 ! rtph264depay ! avdec_h264 ! \
    videoconvert ! jpegenc quality=92 ! \
    multifilesink location=/tmp/precogcam_frame_%03d.jpg max-files=20
else
  exec gst-launch-1.0 \
    udpsrc port=$PORT caps="$CAPS" ! \
    rtpjitterbuffer latency=80 ! rtph264depay ! avdec_h264 ! \
    videoconvert ! osxvideosink
fi
