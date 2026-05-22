import Foundation
import Network
import UIKit

// Wire types — must match temple::WitnessStatsPacket JSON exactly.
// Rust serde serializes struct fields in snake_case by default.
private struct WitnessStatsPacket: Encodable {
    let name: String
    let stats: WitnessStatsPayload
}

private struct WitnessStatsPayload: Encodable {
    let battery_pct: UInt8
    let charging: Bool
    let thermal: String
}

/// Sends battery level + thermal state to REPORT every 2s via UDP.
/// Start after successful registration; stop on disconnect or app background.
final class StatsPublisher {
    private let sourceName: String
    private var timer: DispatchSourceTimer?
    private var connection: NWConnection?

    init(sourceName: String) {
        self.sourceName = sourceName
        UIDevice.current.isBatteryMonitoringEnabled = true
    }

    func start(reportHost: String, statsPort: UInt16) {
        stop()
        let host = NWEndpoint.Host(reportHost)
        guard let port = NWEndpoint.Port(rawValue: statsPort) else { return }
        let conn = NWConnection(host: host, port: port, using: .udp)
        conn.start(queue: .global(qos: .utility))
        connection = conn

        let t = DispatchSource.makeTimerSource(queue: .global(qos: .utility))
        t.schedule(deadline: .now(), repeating: 2.0)
        t.setEventHandler { [weak self] in self?.sendStats() }
        t.resume()
        timer = t
    }

    func stop() {
        timer?.cancel()
        timer = nil
        connection?.cancel()
        connection = nil
    }

    private func sendStats() {
        let level = UIDevice.current.batteryLevel
        let battery: UInt8 = level < 0 ? 0 : UInt8(min(100, Int(level * 100)))
        let state = UIDevice.current.batteryState
        let charging = state == .charging || state == .full
        let thermalStr = thermalStateString(ProcessInfo.processInfo.thermalState)

        let pkt = WitnessStatsPacket(
            name: sourceName,
            stats: WitnessStatsPayload(
                battery_pct: battery,
                charging: charging,
                thermal: thermalStr
            )
        )
        guard let data = try? JSONEncoder().encode(pkt) else { return }
        connection?.send(content: data, completion: .idempotent)
    }

    private func thermalStateString(_ state: ProcessInfo.ThermalState) -> String {
        switch state {
        case .nominal:  return "Nominal"
        case .fair:     return "Fair"
        case .serious:  return "Serious"
        case .critical: return "Critical"
        @unknown default: return "Nominal"
        }
    }

    deinit { stop() }
}
