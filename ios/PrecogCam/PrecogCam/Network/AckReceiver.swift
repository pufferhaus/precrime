import Foundation
import Darwin
import os.log

/// Binds a UDP socket on `0.0.0.0:<port>` and listens for JSON ack packets
/// from REPORT. Fires `onAck` on the main queue for each valid ack received.
///
/// Uses BSD sockets (same approach as RtpSender). Runs a blocking recv loop
/// on a dedicated background thread; a short recv timeout allows clean shutdown.
final class AckReceiver {
    private static let logger = Logger(subsystem: "art.precrime.witness", category: "AckReceiver")

    /// Called on main queue when a valid ack arrives.
    var onAck: ((String) -> Void)?   // reportName

    private let port: UInt16
    private var fd: Int32 = -1
    private var running = false
    private let queue = DispatchQueue(label: "art.precrime.witness.ackReceiver", qos: .utility)

    init(port: UInt16 = 9998) {
        self.port = port
    }

    deinit {
        stop()
    }

    func start() {
        guard !running else { return }

        fd = socket(AF_INET, SOCK_DGRAM, IPPROTO_UDP)
        guard fd >= 0 else {
            Self.logger.error("AckReceiver: socket() failed errno=\(errno)")
            return
        }

        // SO_REUSEADDR so we can restart quickly.
        var reuse: Int32 = 1
        setsockopt(fd, SOL_SOCKET, SO_REUSEADDR, &reuse, socklen_t(MemoryLayout<Int32>.size))

        // Recv timeout — 500ms so the loop can check `running` and exit cleanly.
        var tv = timeval(tv_sec: 0, tv_usec: 500_000)
        setsockopt(fd, SOL_SOCKET, SO_RCVTIMEO, &tv, socklen_t(MemoryLayout<timeval>.size))

        var addr = sockaddr_in()
        addr.sin_family = sa_family_t(AF_INET)
        addr.sin_port = port.bigEndian
        addr.sin_addr = in_addr(s_addr: INADDR_ANY)

        let bindResult = withUnsafePointer(to: &addr) { ptr in
            ptr.withMemoryRebound(to: sockaddr.self, capacity: 1) { sa in
                bind(fd, sa, socklen_t(MemoryLayout<sockaddr_in>.size))
            }
        }

        let listenPort = self.port
        guard bindResult == 0 else {
            Self.logger.error("AckReceiver: bind(::\(listenPort)) failed errno=\(errno)")
            close(fd)
            fd = -1
            return
        }

        running = true
        Self.logger.info("AckReceiver: listening on UDP :\(listenPort)")

        let capturedFd = fd
        queue.async { [weak self] in
            self?.recvLoop(fd: capturedFd)
        }
    }

    func stop() {
        running = false
        if fd >= 0 {
            close(fd)
            fd = -1
        }
    }

    // MARK: - Receive loop (background thread)

    private func recvLoop(fd: Int32) {
        var buf = [UInt8](repeating: 0, count: 2048)

        while running {
            let n = buf.withUnsafeMutableBytes { ptr in
                recv(fd, ptr.baseAddress, ptr.count, 0)
            }

            if n < 0 {
                let e = errno
                if e == EAGAIN || e == EWOULDBLOCK {
                    // Recv timeout expired — check running flag and loop.
                    continue
                }
                if running {
                    Self.logger.error("AckReceiver: recv errno=\(e)")
                }
                break
            }

            if n == 0 { continue }

            let data = Data(buf[0..<n])
            if let reportName = parseAck(data) {
                DispatchQueue.main.async { [weak self] in
                    self?.onAck?(reportName)
                }
            }
        }

        Self.logger.info("AckReceiver: recv loop exited")
    }

    private func parseAck(_ data: Data) -> String? {
        guard let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let v = json["v"] as? String, v == "1",
              let report = json["report"] as? String,
              !report.isEmpty else {
            return nil
        }
        return report
    }
}
