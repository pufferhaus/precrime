import Foundation
import os.log

/// Handles TCP registration with a REPORT receiver.
/// Connects to `host:regPort`, sends one JSON line, reads one JSON response, closes.
///
/// All callbacks arrive on the main queue.
final class RegistrationClient {
    private static let logger = Logger(subsystem: "art.precrime.witness", category: "RegistrationClient")

    struct Registration {
        let assignedPort: UInt16
        let reportName: String
        let ackPort: UInt16
    }

    /// Called on success or failure (main queue).
    var onResult: ((Result<Registration, Error>) -> Void)?

    private let host: String
    private let regPort: Int
    private let timeout: TimeInterval = 5.0

    private var connection: URLSessionStreamTask?
    private let session: URLSession

    init(host: String, regPort: Int) {
        self.host = host
        self.regPort = regPort
        let config = URLSessionConfiguration.default
        config.timeoutIntervalForRequest = 5
        config.timeoutIntervalForResource = 10
        self.session = URLSession(configuration: config)
    }

    /// Build and send the registration JSON, read the response.
    func register(name: String, resolution: String, fps: Int32, bitrateKbps: Int32) {
        let task = session.streamTask(withHostName: host, port: regPort)
        self.connection = task
        task.resume()

        let payload: [String: Any] = [
            "v": "1",
            "name": name,
            "resolution": resolution,
            "fps": fps,
            "bitrate_kbps": bitrateKbps
        ]

        guard let jsonData = try? JSONSerialization.data(withJSONObject: payload),
              var jsonLine = String(data: jsonData, encoding: .utf8) else {
            deliver(.failure(RegistrationError.encodingFailed))
            return
        }
        jsonLine += "\n"

        let writeData = Data(jsonLine.utf8)
        task.write(writeData, timeout: timeout) { [weak self] error in
            if let error {
                self?.deliver(.failure(error))
                return
            }
            self?.readResponse(task: task)
        }
    }

    func cancel() {
        connection?.cancel()
        connection = nil
    }

    // MARK: - Private

    private func readResponse(task: URLSessionStreamTask) {
        // Read up to 4KB — response is a single JSON line.
        task.readData(ofMinLength: 1, maxLength: 4096, timeout: timeout) { [weak self] data, atEOF, error in
            defer { task.closeWrite() }

            if let error {
                self?.deliver(.failure(error))
                return
            }
            guard let data, !data.isEmpty else {
                self?.deliver(.failure(RegistrationError.emptyResponse))
                return
            }

            self?.parseResponse(data)
        }
    }

    private func parseResponse(_ data: Data) {
        guard let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            deliver(.failure(RegistrationError.invalidJSON))
            return
        }

        guard let assignedPort = json["assigned_port"] as? Int,
              assignedPort > 0, assignedPort < 65536,
              let reportName = json["report_name"] as? String,
              let ackPort = json["ack_port"] as? Int,
              ackPort > 0, ackPort < 65536 else {
            deliver(.failure(RegistrationError.missingFields))
            return
        }

        let reg = Registration(
            assignedPort: UInt16(assignedPort),
            reportName: reportName,
            ackPort: UInt16(ackPort)
        )
        deliver(.success(reg))
    }

    private func deliver(_ result: Result<Registration, Error>) {
        DispatchQueue.main.async { [weak self] in
            self?.onResult?(result)
        }
    }
}

// MARK: - Errors

enum RegistrationError: LocalizedError {
    case encodingFailed
    case emptyResponse
    case invalidJSON
    case missingFields

    var errorDescription: String? {
        switch self {
        case .encodingFailed:  return "Failed to encode registration JSON"
        case .emptyResponse:   return "Empty response from REPORT"
        case .invalidJSON:     return "Invalid JSON in registration response"
        case .missingFields:   return "Missing required fields in registration response"
        }
    }
}
