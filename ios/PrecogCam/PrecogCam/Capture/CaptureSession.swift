import Foundation
import AVFoundation
import CoreVideo
import os.log

enum CameraSide: String, CaseIterable, Identifiable {
    case back, front
    var id: String { rawValue }
    var avPosition: AVCaptureDevice.Position { self == .back ? .back : .front }
}

enum CaptureResolution: String, CaseIterable, Identifiable {
    case vga         // 640x480
    case hd720       // 1280x720
    var id: String { rawValue }

    var preset: AVCaptureSession.Preset {
        switch self {
        case .vga: return .vga640x480
        case .hd720: return .hd1280x720
        }
    }
    var label: String {
        switch self {
        case .vga: return "640×480"
        case .hd720: return "1280×720"
        }
    }
}

final class CaptureSession: NSObject, AVCaptureVideoDataOutputSampleBufferDelegate {
    let session = AVCaptureSession()
    private let videoOutput = AVCaptureVideoDataOutput()
    private let captureQueue = DispatchQueue(label: "art.precrime.PrecogCam.capture",
                                              qos: .userInteractive)
    private var currentDevice: AVCaptureDevice?
    private static let logger = Logger(subsystem: "art.precrime.PrecogCam", category: "Capture")

    var onFrame: ((CVPixelBuffer, CMTime) -> Void)?

    private(set) var resolution: CaptureResolution = .vga
    private(set) var fps: Int32 = 30

    /// Hardware limits of the currently active camera.
    var deviceMaxZoom: CGFloat { currentDevice?.activeFormat.videoMaxZoomFactor ?? 1.0 }
    var deviceMinExposureBias: Float { currentDevice?.minExposureTargetBias ?? -2 }
    var deviceMaxExposureBias: Float { currentDevice?.maxExposureTargetBias ?? 2 }

    static func requestCameraPermission() async -> Bool {
        switch AVCaptureDevice.authorizationStatus(for: .video) {
        case .authorized: return true
        case .notDetermined: return await AVCaptureDevice.requestAccess(for: .video)
        default: return false
        }
    }

    func configure(side: CameraSide, resolution: CaptureResolution, fps: Int32) {
        self.resolution = resolution
        self.fps = fps

        session.beginConfiguration()
        defer { session.commitConfiguration() }

        session.sessionPreset = resolution.preset

        for input in session.inputs { session.removeInput(input) }
        for output in session.outputs { session.removeOutput(output) }

        let device = AVCaptureDevice.default(.builtInWideAngleCamera,
                                              for: .video,
                                              position: side.avPosition)
            ?? AVCaptureDevice.default(for: .video)
        guard let device else {
            Self.logger.error("No camera available")
            return
        }
        currentDevice = device

        do {
            let input = try AVCaptureDeviceInput(device: device)
            if session.canAddInput(input) {
                session.addInput(input)
            } else {
                Self.logger.error("Cannot add camera input")
                return
            }
        } catch {
            Self.logger.error("AVCaptureDeviceInput failed: \(error.localizedDescription)")
            return
        }

        do {
            try device.lockForConfiguration()
            let duration = CMTime(value: 1, timescale: CMTimeScale(fps))
            if device.activeFormat.videoSupportedFrameRateRanges.contains(where: {
                $0.minFrameRate <= Double(fps) && Double(fps) <= $0.maxFrameRate
            }) {
                device.activeVideoMinFrameDuration = duration
                device.activeVideoMaxFrameDuration = duration
            } else {
                Self.logger.error("\(fps)fps not supported by device, leaving auto")
            }
            device.unlockForConfiguration()
        } catch {
            Self.logger.error("lockForConfiguration: \(error.localizedDescription)")
        }

        videoOutput.videoSettings = [
            kCVPixelBufferPixelFormatTypeKey as String:
                kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange
        ]
        videoOutput.alwaysDiscardsLateVideoFrames = true
        videoOutput.setSampleBufferDelegate(self, queue: captureQueue)
        if session.canAddOutput(videoOutput) {
            session.addOutput(videoOutput)
        }

        if let connection = videoOutput.connection(with: .video) {
            if connection.isVideoOrientationSupported {
                connection.videoOrientation = .landscapeRight
            }
        }
    }

    /// Apply zoom factor (1.0 = no zoom). Clamped to device's supported range.
    /// Safe to call live; AVCaptureDevice handles smooth ramp internally.
    func applyZoom(_ factor: CGFloat) {
        guard let device = currentDevice else { return }
        let clamped = max(1.0, min(factor, device.activeFormat.videoMaxZoomFactor))
        do {
            try device.lockForConfiguration()
            device.videoZoomFactor = clamped
            device.unlockForConfiguration()
        } catch {
            Self.logger.error("zoom set failed: \(error.localizedDescription)")
        }
    }

    // MARK: - Focus

    /// True if the current device supports tap-to-focus.
    var isFocusLockSupported: Bool {
        guard let device = currentDevice else { return false }
        return device.isFocusPointOfInterestSupported
            && device.isFocusModeSupported(.autoFocus)
            && device.isFocusModeSupported(.locked)
    }

    /// Set focus point and lock. Point is normalised (0–1, origin top-left, AVFoundation convention).
    /// Sets .autoFocus first; after 0.8 s switches to .locked.
    func setFocusPoint(_ point: CGPoint) {
        guard let device = currentDevice,
              device.isFocusPointOfInterestSupported,
              device.isFocusModeSupported(.autoFocus) else { return }
        do {
            try device.lockForConfiguration()
            device.focusPointOfInterest = point
            device.focusMode = .autoFocus
            device.unlockForConfiguration()
        } catch {
            Self.logger.error("setFocusPoint lockForConfiguration: \(error.localizedDescription)")
            return
        }
        // After the AF sweep completes (~0.8 s), pin to .locked.
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.8) { [weak self] in
            guard let device = self?.currentDevice,
                  device.isFocusModeSupported(.locked) else { return }
            do {
                try device.lockForConfiguration()
                device.focusMode = .locked
                device.unlockForConfiguration()
            } catch {
                Self.logger.error("setFocusPoint lock: \(error.localizedDescription)")
            }
        }
    }

    /// Return to continuous autofocus.
    func resetFocus() {
        guard let device = currentDevice,
              device.isFocusModeSupported(.continuousAutoFocus) else { return }
        do {
            try device.lockForConfiguration()
            device.focusMode = .continuousAutoFocus
            device.unlockForConfiguration()
        } catch {
            Self.logger.error("resetFocus lockForConfiguration: \(error.localizedDescription)")
        }
    }

    // MARK: - White balance

    /// True if the current device supports locking white balance.
    var isWhiteBalanceLockSupported: Bool {
        guard let device = currentDevice else { return false }
        return device.isWhiteBalanceModeSupported(.locked)
    }

    /// Lock white balance at the current scene gains.
    func lockWhiteBalance() {
        guard let device = currentDevice,
              device.isWhiteBalanceModeSupported(.locked) else { return }
        do {
            try device.lockForConfiguration()
            device.setWhiteBalanceModeLocked(with: device.deviceWhiteBalanceGains,
                                              completionHandler: nil)
            device.unlockForConfiguration()
        } catch {
            Self.logger.error("lockWhiteBalance: \(error.localizedDescription)")
        }
    }

    /// Return to continuous auto white balance.
    func unlockWhiteBalance() {
        guard let device = currentDevice,
              device.isWhiteBalanceModeSupported(.continuousAutoWhiteBalance) else { return }
        do {
            try device.lockForConfiguration()
            device.whiteBalanceMode = .continuousAutoWhiteBalance
            device.unlockForConfiguration()
        } catch {
            Self.logger.error("unlockWhiteBalance: \(error.localizedDescription)")
        }
    }

    /// Apply exposure target bias in EV. Clamped to device range.
    /// Bias is applied on top of whatever exposure mode is active (default = continuous auto).
    func applyExposureBias(_ ev: Float) {
        guard let device = currentDevice else { return }
        let clamped = max(device.minExposureTargetBias,
                          min(ev, device.maxExposureTargetBias))
        do {
            try device.lockForConfiguration()
            device.setExposureTargetBias(clamped, completionHandler: nil)
            device.unlockForConfiguration()
        } catch {
            Self.logger.error("exposure bias set failed: \(error.localizedDescription)")
        }
    }

    func start() {
        captureQueue.async { [weak self] in
            guard let self, !self.session.isRunning else { return }
            self.session.startRunning()
            Self.logger.info("capture started")
        }
    }

    func stop() {
        captureQueue.async { [weak self] in
            guard let self, self.session.isRunning else { return }
            self.session.stopRunning()
            Self.logger.info("capture stopped")
        }
    }

    // MARK: - AVCaptureVideoDataOutputSampleBufferDelegate

    func captureOutput(_ output: AVCaptureOutput,
                       didOutput sampleBuffer: CMSampleBuffer,
                       from connection: AVCaptureConnection) {
        // Do NOT lock the pixel buffer here — VideoToolbox encodes directly
        // from the CVImageBuffer and may go through the GPU. CPU locking
        // would force a needless readback.
        guard let buffer = CMSampleBufferGetImageBuffer(sampleBuffer) else { return }
        let pts = CMSampleBufferGetPresentationTimeStamp(sampleBuffer)
        onFrame?(buffer, pts)
    }
}
