# PrecogCam — iOS H.264 RTP unicast sender for PRECRIME REPORT
#
#   make project   — generate PrecogCam.xcodeproj via xcodegen
#   make open      — open project in Xcode
#   make clean     — remove generated project
#
# In Xcode: pick Personal Team in Signing & Capabilities, plug phone in, ⌘R.

.PHONY: all project open clean

all: project

project:
	@which xcodegen >/dev/null || (echo "Install: brew install xcodegen"; exit 1)
	xcodegen generate

open: project
	open PrecogCam.xcodeproj

clean:
	rm -rf PrecogCam.xcodeproj build DerivedData
