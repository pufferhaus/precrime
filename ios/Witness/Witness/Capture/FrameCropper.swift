import CoreImage
import CoreVideo

/// Rotates a landscape-oriented CVPixelBuffer 90° clockwise and center-crops
/// to the target output dimensions.
///
/// Used when the phone is mounted portrait with landscapeRight capture orientation:
///   640×480 input → rotate CW → 480×640 → center-crop → 480×270 (16:9)
///   1280×720 input → rotate CW → 720×1280 → center-crop → 720×405 (16:9)
final class FrameCropper {
    private let context: CIContext
    private let pool: CVPixelBufferPool?
    let outputWidth: Int
    let outputHeight: Int

    init(outputWidth: Int, outputHeight: Int) {
        self.outputWidth = outputWidth
        self.outputHeight = outputHeight
        context = CIContext(options: [
            .workingColorSpace: NSNull(),
            .outputColorSpace: NSNull()
        ])
        var p: CVPixelBufferPool?
        let attrs: [CFString: Any] = [
            kCVPixelBufferWidthKey: outputWidth,
            kCVPixelBufferHeightKey: outputHeight,
            kCVPixelBufferPixelFormatTypeKey: kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange,
            kCVPixelBufferIOSurfacePropertiesKey: [:] as [String: Any]
        ]
        CVPixelBufferPoolCreate(nil, nil, attrs as CFDictionary, &p)
        pool = p
    }

    /// Rotates the input 90° CW then center-crops to `outputWidth × outputHeight`.
    /// Returns nil if the pixel buffer pool is exhausted (caller falls back to raw frame).
    func crop(_ input: CVPixelBuffer) -> CVPixelBuffer? {
        guard let pool else { return nil }

        // .right = EXIF 6: source pixels are 90° CCW from natural → correct by rotating 90° CW.
        let ci = CIImage(cvPixelBuffer: input).oriented(.right)
        let ext = ci.extent

        // Center-crop origin in CIImage coordinate space (y-up, may be offset after orientation).
        let cropX = ext.minX + (ext.width  - CGFloat(outputWidth))  / 2
        let cropY = ext.minY + (ext.height - CGFloat(outputHeight)) / 2

        // Translate the crop region to (0,0) so context.render fills the output buffer
        // without needing to know the oriented CIImage's internal coordinate offset.
        let normalized = ci.transformed(by: CGAffineTransform(translationX: -cropX, y: -cropY))

        var out: CVPixelBuffer?
        CVPixelBufferPoolCreatePixelBuffer(nil, pool, &out)
        guard let out else { return nil }

        context.render(normalized, to: out,
                       bounds: CGRect(x: 0, y: 0,
                                      width:  CGFloat(outputWidth),
                                      height: CGFloat(outputHeight)),
                       colorSpace: nil)
        return out
    }
}
