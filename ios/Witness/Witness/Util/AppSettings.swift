import Foundation
import Combine
import UIKit

final class AppSettings: ObservableObject {
    @Published var sourceName: String {
        didSet { defaults.set(sourceName, forKey: Keys.sourceName) }
    }
    @Published var targetHost: String {
        didSet { defaults.set(targetHost, forKey: Keys.targetHost) }
    }
    @Published var targetPort: Int32 {
        didSet { defaults.set(Int(targetPort), forKey: Keys.targetPort) }
    }
    @Published var resolution: CaptureResolution {
        didSet { defaults.set(resolution.rawValue, forKey: Keys.resolution) }
    }
    @Published var fps: Int32 {
        didSet { defaults.set(Int(fps), forKey: Keys.fps) }
    }
    @Published var bitrateKbps: Int32 {
        didSet { defaults.set(Int(bitrateKbps), forKey: Keys.bitrateKbps) }
    }
    @Published var cameraSide: CameraSide {
        didSet { defaults.set(cameraSide.rawValue, forKey: Keys.cameraSide) }
    }
    @Published var zoomFactor: Double {
        didSet { defaults.set(zoomFactor, forKey: Keys.zoomFactor) }
    }
    @Published var exposureBias: Double {
        didSet { defaults.set(exposureBias, forKey: Keys.exposureBias) }
    }
    @Published var stageMode: Bool {
        didSet { defaults.set(stageMode, forKey: Keys.stageMode) }
    }
    @Published var kioskMode: Bool {
        didSet { defaults.set(kioskMode, forKey: Keys.kioskMode) }
    }

    private let defaults = UserDefaults.standard

    private enum Keys {
        static let sourceName = "sourceName"
        static let targetHost = "targetHost"
        static let targetPort = "targetPort"
        static let resolution = "resolution"
        static let fps = "fps"
        static let bitrateKbps = "bitrateKbps"
        static let cameraSide = "cameraSide"
        static let zoomFactor = "zoomFactor"
        static let exposureBias = "exposureBias"
        static let stageMode = "stageMode"
        static let kioskMode = "kioskMode"
    }

    init() {
        self.sourceName = defaults.string(forKey: Keys.sourceName) ?? Self.defaultName()
        self.targetHost = defaults.string(forKey: Keys.targetHost) ?? "192.168.86.21"
        let storedPort = defaults.integer(forKey: Keys.targetPort)
        self.targetPort = storedPort == 0 ? 5000 : Int32(storedPort)
        if let raw = defaults.string(forKey: Keys.resolution),
           let r = CaptureResolution(rawValue: raw) {
            self.resolution = r
        } else {
            self.resolution = .vga
        }
        let storedFps = defaults.integer(forKey: Keys.fps)
        self.fps = storedFps == 0 ? 30 : Int32(storedFps)
        let storedBr = defaults.integer(forKey: Keys.bitrateKbps)
        self.bitrateKbps = storedBr == 0 ? 2000 : Int32(storedBr)
        if let raw = defaults.string(forKey: Keys.cameraSide),
           let s = CameraSide(rawValue: raw) {
            self.cameraSide = s
        } else {
            self.cameraSide = .back
        }
        let storedZoom = defaults.double(forKey: Keys.zoomFactor)
        self.zoomFactor = storedZoom < 1.0 ? 1.0 : storedZoom
        self.exposureBias = defaults.double(forKey: Keys.exposureBias)
        self.stageMode = defaults.bool(forKey: Keys.stageMode)
        self.kioskMode = defaults.bool(forKey: Keys.kioskMode)
    }

    private static func defaultName() -> String {
        let id = UIDevice.current.identifierForVendor?.uuidString ?? "00000000"
        let suffix = id.replacingOccurrences(of: "-", with: "").prefix(4).uppercased()
        return "PRECOG-\(suffix)-CAM"
    }
}
