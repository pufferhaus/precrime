import Foundation
import CoreMedia
import CoreVideo
import VideoToolbox
import os.log

/// VideoToolbox H.264 encoder. NV12 CVPixelBuffer in, raw NALU bytes out.
/// Real-time mode, no B-frames, ~1s IDR interval.
///
/// NALU output is **AVCC-style payload** (the raw bytes, no length prefix and
/// no Annex-B start codes). The RTP packetizer is the consumer; RTP wants the
/// bare NALU. For keyframes, SPS and PPS are extracted from the format
/// description and prepended to the NALU list.
final class H264Encoder {
    typealias OutputCallback = (_ nalus: [Data], _ pts: CMTime, _ isKeyframe: Bool) -> Void

    var onOutput: OutputCallback?

    private var session: VTCompressionSession?
    private let width: Int32
    private let height: Int32
    private let fps: Int32
    private let bitrateBps: Int32
    private var firstFrameSent = false
    private static let logger = Logger(subsystem: "art.precrime.PrecogCam", category: "H264Encoder")

    init(width: Int32, height: Int32, fps: Int32, bitrateBps: Int32) throws {
        self.width = width
        self.height = height
        self.fps = fps
        self.bitrateBps = bitrateBps
        try createSession()
    }

    deinit {
        if let s = session {
            VTCompressionSessionInvalidate(s)
        }
    }

    private func createSession() throws {
        var s: VTCompressionSession?
        let status = VTCompressionSessionCreate(
            allocator: kCFAllocatorDefault,
            width: width,
            height: height,
            codecType: kCMVideoCodecType_H264,
            encoderSpecification: nil,
            imageBufferAttributes: nil,
            compressedDataAllocator: nil,
            outputCallback: Self.cCallback,
            refcon: Unmanaged.passUnretained(self).toOpaque(),
            compressionSessionOut: &s
        )
        guard status == noErr, let session = s else {
            throw NSError(
                domain: "H264Encoder",
                code: Int(status),
                userInfo: [NSLocalizedDescriptionKey: "VTCompressionSessionCreate failed \(status)"]
            )
        }
        self.session = session

        VTSessionSetProperty(session, key: kVTCompressionPropertyKey_RealTime, value: kCFBooleanTrue)
        VTSessionSetProperty(session, key: kVTCompressionPropertyKey_AllowFrameReordering, value: kCFBooleanFalse)
        VTSessionSetProperty(session, key: kVTCompressionPropertyKey_ProfileLevel, value: kVTProfileLevel_H264_Baseline_AutoLevel)
        VTSessionSetProperty(session, key: kVTCompressionPropertyKey_AverageBitRate, value: NSNumber(value: bitrateBps))
        VTSessionSetProperty(session, key: kVTCompressionPropertyKey_MaxKeyFrameInterval, value: NSNumber(value: fps))
        VTSessionSetProperty(session, key: kVTCompressionPropertyKey_MaxKeyFrameIntervalDuration, value: NSNumber(value: 1.0))
        VTSessionSetProperty(session, key: kVTCompressionPropertyKey_ExpectedFrameRate, value: NSNumber(value: fps))

        VTCompressionSessionPrepareToEncodeFrames(session)
        Self.logger.info("H264Encoder ready \(self.width)x\(self.height)@\(self.fps) bps=\(self.bitrateBps)")
    }

    func encode(pixelBuffer: CVPixelBuffer, pts: CMTime) {
        guard let session else { return }
        let duration = CMTime(value: 1, timescale: CMTimeScale(fps))
        var flags: VTEncodeInfoFlags = []

        // Force IDR on first frame so the receiver can start decoding
        // immediately instead of waiting up to 1s for the scheduled keyframe.
        var props: CFDictionary?
        if !firstFrameSent {
            props = [kVTEncodeFrameOptionKey_ForceKeyFrame: kCFBooleanTrue] as CFDictionary
            firstFrameSent = true
        }

        let status = VTCompressionSessionEncodeFrame(
            session,
            imageBuffer: pixelBuffer,
            presentationTimeStamp: pts,
            duration: duration,
            frameProperties: props,
            sourceFrameRefcon: nil,
            infoFlagsOut: &flags
        )
        if status != noErr {
            Self.logger.error("EncodeFrame status=\(status)")
        }
    }

    // MARK: - C callback bridge

    private static let cCallback: VTCompressionOutputCallback = { refcon, _, status, _, sampleBuffer in
        guard status == noErr,
              let sampleBuffer = sampleBuffer,
              let refcon = refcon else { return }
        let encoder = Unmanaged<H264Encoder>.fromOpaque(refcon).takeUnretainedValue()
        encoder.handle(sampleBuffer)
    }

    private func handle(_ sb: CMSampleBuffer) {
        let pts = CMSampleBufferGetPresentationTimeStamp(sb)
        let isKey = sampleBufferIsKeyframe(sb)

        var nalus: [Data] = []

        // Prepend SPS + PPS on each keyframe so a fresh receiver can decode
        // without out-of-band SDP.
        if isKey, let fmt = CMSampleBufferGetFormatDescription(sb) {
            if let sps = parameterSet(fmt, index: 0) { nalus.append(sps) }
            if let pps = parameterSet(fmt, index: 1) { nalus.append(pps) }
        }

        // Extract NALUs from AVCC-formatted CMBlockBuffer:
        // [4-byte BE length][NALU bytes][4-byte BE length][NALU bytes]...
        guard let block = CMSampleBufferGetDataBuffer(sb) else { return }
        var totalLength = 0
        var dataPointer: UnsafeMutablePointer<Int8>?
        let st = CMBlockBufferGetDataPointer(
            block,
            atOffset: 0,
            lengthAtOffsetOut: nil,
            totalLengthOut: &totalLength,
            dataPointerOut: &dataPointer
        )
        guard st == kCMBlockBufferNoErr, let base = dataPointer else { return }

        let basePtr = UnsafeRawPointer(base)
        var offset = 0
        while offset + 4 <= totalLength {
            // Read 4-byte big-endian length (use loadUnaligned for safety).
            let lenBE = basePtr.advanced(by: offset).loadUnaligned(as: UInt32.self)
            let len = Int(UInt32(bigEndian: lenBE))
            offset += 4
            if len == 0 || offset + len > totalLength { break }
            let nalu = Data(bytes: basePtr.advanced(by: offset), count: len)
            nalus.append(nalu)
            offset += len
        }

        onOutput?(nalus, pts, isKey)
    }

    private func sampleBufferIsKeyframe(_ sb: CMSampleBuffer) -> Bool {
        guard let attachments = CMSampleBufferGetSampleAttachmentsArray(sb, createIfNecessary: false),
              CFArrayGetCount(attachments) > 0 else {
            return true
        }
        let raw = CFArrayGetValueAtIndex(attachments, 0)
        let dict = unsafeBitCast(raw, to: CFDictionary.self)
        let key = Unmanaged.passUnretained(kCMSampleAttachmentKey_NotSync).toOpaque()
        guard let notSyncRaw = CFDictionaryGetValue(dict, key) else {
            return true  // attachment absent → assumed keyframe
        }
        let notSync = unsafeBitCast(notSyncRaw, to: CFBoolean.self)
        return !CFBooleanGetValue(notSync)
    }

    private func parameterSet(_ fmt: CMFormatDescription, index: Int) -> Data? {
        var ptr: UnsafePointer<UInt8>?
        var size = 0
        var count = 0
        let status = CMVideoFormatDescriptionGetH264ParameterSetAtIndex(
            fmt,
            parameterSetIndex: index,
            parameterSetPointerOut: &ptr,
            parameterSetSizeOut: &size,
            parameterSetCountOut: &count,
            nalUnitHeaderLengthOut: nil
        )
        guard status == noErr, let p = ptr else { return nil }
        return Data(bytes: p, count: size)
    }
}
