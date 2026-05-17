import Foundation
import os.log

/// Browses for `_precrime-report._tcp` on the local network.
/// When a service is found and resolved, fires `onFound(host:regPort:)` on the main queue.
/// If no report is found within `timeout` seconds, fires `onTimeout()`.
///
/// Call `startBrowsing()` to begin. Call `stop()` to tear down.
/// The object is single-use after `stop()`; create a new one to retry.
final class ReportDiscovery: NSObject {
    private static let logger = Logger(subsystem: "art.precrime.PrecogCam", category: "ReportDiscovery")

    var onFound: ((String, Int) -> Void)?   // (host, regPort) on main queue
    var onTimeout: (() -> Void)?             // fired if no report found within timeout

    private let timeout: TimeInterval
    private var browser: NetServiceBrowser?
    private var pendingServices: [NetService] = []
    private var timeoutTimer: Timer?
    private var found = false

    init(timeout: TimeInterval = 15) {
        self.timeout = timeout
        super.init()
    }

    deinit {
        stop()
    }

    func startBrowsing() {
        stop()
        found = false

        let b = NetServiceBrowser()
        b.delegate = self
        b.schedule(in: .main, forMode: .common)
        b.searchForServices(ofType: "_precrime-report._tcp.", inDomain: "local.")
        browser = b

        timeoutTimer = Timer.scheduledTimer(withTimeInterval: timeout, repeats: false) { [weak self] _ in
            guard let self, !self.found else { return }
            Self.logger.info("no report found within \(self.timeout)s timeout")
            self.onTimeout?()
        }

        Self.logger.info("browsing for _precrime-report._tcp")
    }

    func stop() {
        timeoutTimer?.invalidate()
        timeoutTimer = nil
        browser?.stop()
        browser = nil
        for svc in pendingServices {
            svc.stop()
        }
        pendingServices = []
    }
}

// MARK: - NetServiceBrowserDelegate

extension ReportDiscovery: NetServiceBrowserDelegate {
    func netServiceBrowser(_ browser: NetServiceBrowser,
                           didFind service: NetService,
                           moreComing: Bool) {
        Self.logger.info("found service: '\(service.name, privacy: .public)'")
        service.delegate = self
        service.schedule(in: .main, forMode: .common)
        pendingServices.append(service)
        service.resolve(withTimeout: 10)
    }

    func netServiceBrowser(_ browser: NetServiceBrowser,
                           didRemove service: NetService,
                           moreComing: Bool) {
        Self.logger.info("service removed: '\(service.name, privacy: .public)'")
    }

    func netServiceBrowser(_ browser: NetServiceBrowser,
                           didNotSearch errorDict: [String: NSNumber]) {
        Self.logger.error("browse failed: \(errorDict, privacy: .public)")
    }
}

// MARK: - NetServiceDelegate

extension ReportDiscovery: NetServiceDelegate {
    func netServiceDidResolveAddress(_ sender: NetService) {
        guard !found else { return }

        // Extract hostname from resolved addresses.
        guard let host = extractHost(from: sender) else {
            Self.logger.error("could not extract host from '\(sender.name, privacy: .public)'")
            return
        }

        // Extract reg_port from TXT record, defaulting to 4999.
        var regPort = 4999
        if let txtData = sender.txtRecordData() {
            let txt = NetService.dictionary(fromTXTRecord: txtData)
            if let portData = txt["reg_port"],
               let portStr = String(data: portData, encoding: .utf8),
               let port = Int(portStr), port > 0 {
                regPort = port
            }
        }

        found = true
        timeoutTimer?.invalidate()
        timeoutTimer = nil

        Self.logger.info("resolved '\(sender.name, privacy: .public)' → \(host, privacy: .public):\(regPort)")
        DispatchQueue.main.async { [weak self] in
            self?.onFound?(host, regPort)
        }
    }

    func netService(_ sender: NetService, didNotResolve errorDict: [String: NSNumber]) {
        Self.logger.error("resolve failed for '\(sender.name, privacy: .public)': \(errorDict, privacy: .public)")
        pendingServices.removeAll { $0 === sender }
    }

    // MARK: - Helpers

    private func extractHost(from service: NetService) -> String? {
        // Prefer hostName (available after resolve).
        if let h = service.hostName, !h.isEmpty {
            // Strip trailing dot from mDNS hostname.
            let clean = h.hasSuffix(".") ? String(h.dropLast()) : h
            return clean
        }
        // Fall back to parsing addresses.
        guard let addresses = service.addresses else { return nil }
        for addrData in addresses {
            let host = addrData.withUnsafeBytes { ptr -> String? in
                guard let baseAddr = ptr.baseAddress else { return nil }
                let sa = baseAddr.assumingMemoryBound(to: sockaddr.self)
                if sa.pointee.sa_family == sa_family_t(AF_INET) {
                    var buf = [CChar](repeating: 0, count: Int(INET_ADDRSTRLEN))
                    let sin = baseAddr.assumingMemoryBound(to: sockaddr_in.self)
                    var inAddr = sin.pointee.sin_addr
                    inet_ntop(AF_INET, &inAddr, &buf, socklen_t(INET_ADDRSTRLEN))
                    return String(cString: buf)
                }
                return nil
            }
            if let h = host { return h }
        }
        return nil
    }
}
