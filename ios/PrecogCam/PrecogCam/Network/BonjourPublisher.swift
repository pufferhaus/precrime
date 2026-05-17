import Foundation
import os.log

/// Publishes `_precog._tcp` via NetService so this camera source is visible
/// on the LAN. Operators can verify presence with `dns-sd -B _precog._tcp local.`
/// from any Mac on the same network.
///
/// TXT record carries v, resolution, and fps — informational for management
/// tooling; REPORT does not actively browse this service.
final class BonjourPublisher: NSObject {
    private static let logger = Logger(subsystem: "art.precrime.witness", category: "BonjourPublisher")

    private var service: NetService?
    private var name: String
    private var resolution: String      // ASCII "640x480"
    private var fps: Int32

    init(name: String, resolution: String, fps: Int32) {
        self.name = name
        self.resolution = resolution
        self.fps = fps
        super.init()
    }

    deinit {
        stop()
    }

    func start() {
        publish()
    }

    func stop() {
        service?.stop()
        service = nil
    }

    /// Call when sourceName, resolution, or fps changes.
    func republish(name: String, resolution: String, fps: Int32) {
        self.name = name
        self.resolution = resolution
        self.fps = fps
        stop()
        publish()
    }

    // MARK: - Private

    private func publish() {
        let serviceName = self.name
        let svc = NetService(domain: "local.", type: "_precog._tcp.", name: serviceName, port: 0)
        svc.delegate = self
        svc.setTXTRecord(makeTXT())
        svc.schedule(in: .main, forMode: .common)
        svc.publish()
        service = svc
        Self.logger.info("publishing _precog._tcp '\(serviceName, privacy: .public)'")
    }

    private func makeTXT() -> Data {
        let dict: [String: Data] = [
            "v": Data("1".utf8),
            "resolution": Data(resolution.utf8),
            "fps": Data("\(fps)".utf8)
        ]
        return NetService.data(fromTXTRecord: dict)
    }
}

extension BonjourPublisher: NetServiceDelegate {
    func netServiceDidPublish(_ sender: NetService) {
        Self.logger.info("_precog._tcp published OK: '\(sender.name, privacy: .public)'")
    }

    func netService(_ sender: NetService, didNotPublish errorDict: [String: NSNumber]) {
        Self.logger.error("_precog._tcp publish failed: \(errorDict, privacy: .public)")
    }
}
