import SwiftUI
import AVFoundation

struct CameraPreview: UIViewRepresentable {
    let session: AVCaptureSession
    /// Called on single-tap. Provides (devicePoint, viewPoint) where devicePoint is
    /// normalised AVFoundation coords (0–1, origin top-left) and viewPoint is in the
    /// UIView's coordinate space (suitable for placing an overlay).
    var onFocusTap: ((_ devicePoint: CGPoint, _ viewPoint: CGPoint) -> Void)?

    func makeCoordinator() -> Coordinator {
        Coordinator(onFocusTap: onFocusTap)
    }

    func makeUIView(context: Context) -> PreviewView {
        let v = PreviewView()
        v.videoPreviewLayer.session = session
        v.videoPreviewLayer.videoGravity = .resizeAspect
        v.backgroundColor = .black

        let tap = UITapGestureRecognizer(target: context.coordinator,
                                         action: #selector(Coordinator.handleTap(_:)))
        tap.numberOfTapsRequired = 1
        v.addGestureRecognizer(tap)

        return v
    }

    func updateUIView(_ uiView: PreviewView, context: Context) {
        // Update the callback in case the closure captures changed state.
        context.coordinator.onFocusTap = onFocusTap
    }

    // MARK: - Coordinator

    final class Coordinator: NSObject {
        var onFocusTap: ((_ devicePoint: CGPoint, _ viewPoint: CGPoint) -> Void)?

        init(onFocusTap: ((_ devicePoint: CGPoint, _ viewPoint: CGPoint) -> Void)?) {
            self.onFocusTap = onFocusTap
        }

        @objc func handleTap(_ recognizer: UITapGestureRecognizer) {
            guard let view = recognizer.view as? PreviewView,
                  let callback = onFocusTap else { return }
            let viewPoint = recognizer.location(in: view)
            let devicePoint = view.videoPreviewLayer
                .captureDevicePointConverted(fromLayerPoint: viewPoint)
            callback(devicePoint, viewPoint)
        }
    }

    // MARK: - PreviewView

    final class PreviewView: UIView {
        override class var layerClass: AnyClass { AVCaptureVideoPreviewLayer.self }
        var videoPreviewLayer: AVCaptureVideoPreviewLayer {
            layer as! AVCaptureVideoPreviewLayer
        }
    }
}
