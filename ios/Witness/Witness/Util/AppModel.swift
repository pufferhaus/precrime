import Foundation
import UIKit
import CoreMedia
import CoreVideo
import Combine
import Darwin
import os.log

// MARK: - Camera control state

enum FocusState: Equatable {
    case continuous
    case focusing(point: CGPoint)
    case locked(point: CGPoint)
}

enum WhiteBalanceState { case auto, locked }

// MARK: - Connection state

enum ConnectionState: Equatable {
    case searching
    case registering
    case streaming                      // RTP sending, no ack yet
    case live(reportName: String)       // acks flowing
    case lost(reportName: String)       // ack timeout, re-searching
}

// MARK: - AppModel

@MainActor
final class AppModel: ObservableObject {
    @Published var permissionGranted = false
    @Published var isStreaming = false
    @Published var lastError: String?
    @Published var sentPackets: UInt64 = 0
    @Published var sentBytes: UInt64 = 0
    @Published var localIP: String = "—"
    @Published var connectionState: ConnectionState = .searching
    @Published var focusState: FocusState = .continuous
    @Published var whiteBalanceState: WhiteBalanceState = .auto

    // Populated from registration response (read-only display).
    @Published var connectedReportName: String = ""
    @Published var connectedPort: UInt16 = 0

    let settings: AppSettings
    let capture = CaptureSession()
    private let hot = HotState()
    private var settingsObservers: [AnyCancellable] = []
    private var statsTimer: AnyCancellable?

    // Network layer objects.
    private var bonjourPublisher: BonjourPublisher?
    private var reportDiscovery: ReportDiscovery?
    private var registrationClient: RegistrationClient?
    private var ackReceiver: AckReceiver?

    // Keep-alive re-registration.
    private var keepAliveTimer: Timer?
    private var ackWatchTimer: Timer?
    private var lastAckAt: Date?
    private var currentReportHost: String?
    private var currentRegPort: Int = 4999

    private static let logger = Logger(subsystem: "art.precrime.witness", category: "AppModel")

    init(settings: AppSettings = AppSettings()) {
        self.settings = settings
        let hot = self.hot
        capture.onFrame = { pixelBuffer, pts in
            hot.encoder?.encode(pixelBuffer: pixelBuffer, pts: pts)
        }

        settingsObservers.append(settings.$zoomFactor.sink { [weak self] z in
            self?.capture.applyZoom(CGFloat(z))
        })
        settingsObservers.append(settings.$exposureBias.sink { [weak self] ev in
            self?.capture.applyExposureBias(Float(ev))
        })
        settingsObservers.append(settings.$stageMode.sink { [weak self] on in
            self?.applyStageMode(on)
        })
        // When source identity changes, republish Bonjour TXT.
        settingsObservers.append(settings.$sourceName.dropFirst().sink { [weak self] _ in
            self?.republishBonjour()
        })
        settingsObservers.append(settings.$resolution.dropFirst().sink { [weak self] _ in
            self?.republishBonjour()
        })
        settingsObservers.append(settings.$fps.dropFirst().sink { [weak self] _ in
            self?.republishBonjour()
        })

        // Refresh stats + local IP every second for stage status display.
        statsTimer = Timer.publish(every: 1, on: .main, in: .common)
            .autoconnect()
            .sink { [weak self] _ in
                guard let self else { return }
                if let sender = self.hot.sender {
                    self.sentPackets = sender.packetsSent
                    self.sentBytes = sender.bytesSent
                }
                self.localIP = Self.wifiIP() ?? "—"
            }
    }

    // MARK: - Stage mode

    private func applyStageMode(_ on: Bool) {
        UIScreen.main.brightness = on ? 0.15 : 0.6
    }

    // MARK: - Bootstrap

    func bootstrap() async {
        permissionGranted = await CaptureSession.requestCameraPermission()
        guard permissionGranted else {
            lastError = "Camera permission denied"
            return
        }
        UIApplication.shared.isIdleTimerDisabled = true
        startEncodePipeline()
        startNetworkDiscovery()
    }

    // MARK: - Encode pipeline (camera + encoder, no RTP target yet)

    private func startEncodePipeline() {
        guard hot.encoder == nil else { return }
        capture.configure(
            side: settings.cameraSide,
            resolution: settings.resolution,
            fps: settings.fps
        )

        let (w, h) = nativeDimensions(for: settings.resolution)
        let encoder: H264Encoder
        do {
            encoder = try H264Encoder(
                width: w, height: h,
                fps: settings.fps,
                bitrateBps: settings.bitrateKbps * 1000
            )
        } catch {
            lastError = "Encoder init failed: \(error.localizedDescription)"
            return
        }

        let packetizer = RtpPacketizer(ssrc: Self.ssrcFromName(settings.sourceName))
        // Create sender with no target — sends are no-ops until retarget().
        let sender = RtpSender()

        encoder.onOutput = { nalus, pts, _ in
            let packets = packetizer.packetize(nalus: nalus, pts: pts)
            sender.sendBatch(packets)
        }

        hot.encoder = encoder
        hot.packetizer = packetizer
        hot.sender = sender

        capture.start()
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.2) { [weak self] in
            guard let self else { return }
            self.capture.applyZoom(CGFloat(self.settings.zoomFactor))
            self.capture.applyExposureBias(Float(self.settings.exposureBias))
        }
        isStreaming = true
        Self.logger.info("encode pipeline started: \(self.settings.resolution.label)/\(self.settings.fps)fps \(self.settings.bitrateKbps)kbps")
    }

    private func stopEncodePipeline() {
        capture.stop()
        hot.sender = nil
        hot.packetizer = nil
        hot.encoder = nil
        isStreaming = false
    }

    // MARK: - Network discovery

    private func startNetworkDiscovery() {
        // Publish our own presence.
        let publisher = BonjourPublisher(
            name: settings.sourceName,
            resolution: resolutionString(),
            fps: settings.fps
        )
        publisher.start()
        bonjourPublisher = publisher

        // Browse for REPORT.
        connectionState = .searching
        beginBrowsing()
    }

    private func beginBrowsing() {
        reportDiscovery?.stop()

        let discovery = ReportDiscovery(timeout: 15)
        discovery.onFound = { [weak self] host, regPort in
            self?.onReportFound(host: host, regPort: regPort)
        }
        discovery.onTimeout = { [weak self] in
            self?.onDiscoveryTimeout()
        }
        discovery.startBrowsing()
        reportDiscovery = discovery
    }

    private func onReportFound(host: String, regPort: Int) {
        currentReportHost = host
        currentRegPort = regPort
        Self.logger.info("report found: \(host, privacy: .public):\(regPort)")
        doRegistration(host: host, regPort: regPort)
    }

    private func onDiscoveryTimeout() {
        // Fallback: if targetHost is manually set, try it directly.
        if !settings.targetHost.isEmpty {
            Self.logger.info("discovery timeout — falling back to manual host \(self.settings.targetHost, privacy: .public)")
            let host = settings.targetHost
            let regPort = Int(settings.targetPort > 0 ? settings.targetPort : 5000)
            currentReportHost = host
            currentRegPort = regPort
            doRegistration(host: host, regPort: regPort)
        } else {
            Self.logger.info("discovery timeout — no manual fallback, retrying browse")
            beginBrowsing()
        }
    }

    // MARK: - Registration

    /// Initial registration: updates connection state and starts streaming machinery.
    private func doRegistration(host: String, regPort: Int) {
        connectionState = .registering
        sendRegistration(host: host, regPort: regPort, isKeepAlive: false)
    }

    /// Keep-alive re-registration: silent — does not change visible state.
    private func keepAliveRegistration(host: String, regPort: Int) {
        sendRegistration(host: host, regPort: regPort, isKeepAlive: true)
    }

    private func sendRegistration(host: String, regPort: Int, isKeepAlive: Bool) {
        let client = RegistrationClient(host: host, regPort: regPort)
        client.onResult = { [weak self] result in
            self?.onRegistrationResult(result, isKeepAlive: isKeepAlive)
        }
        client.register(
            name: settings.sourceName,
            resolution: resolutionString(),
            fps: settings.fps,
            bitrateKbps: settings.bitrateKbps
        )
        registrationClient = client
        let tag = isKeepAlive ? "keep-alive" : "initial"
        Self.logger.info("\(tag, privacy: .public) registration with \(host, privacy: .public):\(regPort)")
    }

    private func onRegistrationResult(_ result: Result<RegistrationClient.Registration, Error>, isKeepAlive: Bool) {
        switch result {
        case .success(let reg):
            Self.logger.info("registered: port=\(reg.assignedPort) report='\(reg.reportName, privacy: .public)' ackPort=\(reg.ackPort) keepAlive=\(isKeepAlive)")
            connectedReportName = reg.reportName
            connectedPort = reg.assignedPort

            // Only update sender/ackReceiver/state when not in a stable live state
            // (i.e., initial or recovering), or if the endpoint actually changed.
            let newHost = currentReportHost ?? ""
            let senderNeedsRetarget = !isKeepAlive
                || hot.sender?.host != newHost
                || hot.sender?.port != reg.assignedPort
            let ackNeedsRestart = !isKeepAlive

            if senderNeedsRetarget, !newHost.isEmpty {
                hot.sender?.retarget(host: newHost, port: reg.assignedPort)
            }

            if ackNeedsRestart {
                ackReceiver?.stop()
                let ack = AckReceiver(port: reg.ackPort)
                ack.onAck = { [weak self] reportName in
                    self?.onAckReceived(reportName: reportName)
                }
                ack.start()
                ackReceiver = ack

                // Only advance to .streaming (not already live) on initial registration.
                if case .live = connectionState {
                    // Already live — leave state alone.
                } else {
                    connectionState = .streaming
                }

                // Restart ack watchdog.
                startAckWatchdog()

                // Arm keep-alive re-registration timer (every 15s).
                keepAliveTimer?.invalidate()
                keepAliveTimer = Timer.scheduledTimer(withTimeInterval: 15, repeats: true) { [weak self] _ in
                    self?.keepAliveReregister()
                }
            }

        case .failure(let error):
            Self.logger.error("registration failed (keepAlive=\(isKeepAlive)): \(error.localizedDescription, privacy: .public)")
            if isKeepAlive {
                // Retry keep-alive after 3s without changing state.
                DispatchQueue.main.asyncAfter(deadline: .now() + 3) { [weak self] in
                    guard let self, let host = self.currentReportHost else { return }
                    self.keepAliveRegistration(host: host, regPort: self.currentRegPort)
                }
            } else {
                // Initial registration failed — retry after 3s.
                DispatchQueue.main.asyncAfter(deadline: .now() + 3) { [weak self] in
                    guard let self else { return }
                    if let host = self.currentReportHost {
                        self.doRegistration(host: host, regPort: self.currentRegPort)
                    } else {
                        self.connectionState = .searching
                        self.beginBrowsing()
                    }
                }
            }
        }
    }

    // MARK: - Ack handling

    private func onAckReceived(reportName: String) {
        lastAckAt = Date()
        switch connectionState {
        case .streaming, .lost:
            connectionState = .live(reportName: reportName)
            Self.logger.info("state → LIVE '\(reportName, privacy: .public)'")
        case .live:
            break   // Already live — just update the timestamp.
        default:
            break
        }
    }

    private func startAckWatchdog() {
        ackWatchTimer?.invalidate()
        lastAckAt = nil
        ackWatchTimer = Timer.scheduledTimer(withTimeInterval: 1, repeats: true) { [weak self] _ in
            self?.checkAckTimeout()
        }
    }

    private func checkAckTimeout() {
        let reportName: String
        switch connectionState {
        case .live(let name):
            reportName = name
        case .streaming:
            reportName = connectedReportName
        default:
            return
        }

        guard let last = lastAckAt else {
            // In streaming but no ack yet — no timeout applies yet.
            return
        }
        if Date().timeIntervalSince(last) > 6 {
            Self.logger.info("ack timeout → LOST '\(reportName, privacy: .public)'")
            connectionState = .lost(reportName: reportName)
            restartDiscovery()
        }
    }

    // MARK: - Keep-alive re-registration

    private func keepAliveReregister() {
        guard let host = currentReportHost,
              case .live = connectionState else { return }
        let regPort = self.currentRegPort
        Self.logger.info("keep-alive re-registration with \(host, privacy: .public):\(regPort)")
        keepAliveRegistration(host: host, regPort: regPort)
    }

    // MARK: - Discovery restart

    private func restartDiscovery() {
        keepAliveTimer?.invalidate()
        keepAliveTimer = nil
        ackWatchTimer?.invalidate()
        ackWatchTimer = nil
        ackReceiver?.stop()
        ackReceiver = nil
        currentReportHost = nil
        hot.sender?.untarget()  // Stop sending until re-registered.
        // Note: we leave isStreaming true; the pipeline is alive.
        connectionState = .searching
        beginBrowsing()
    }

    // MARK: - Camera controls

    /// Set a tap-to-focus point. Point is normalised device coords (0–1 origin top-left).
    func setFocusPoint(_ devicePoint: CGPoint) {
        focusState = .focusing(point: devicePoint)
        capture.setFocusPoint(devicePoint)
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.8) { [weak self] in
            guard let self else { return }
            if case .focusing = self.focusState {
                self.focusState = .locked(point: devicePoint)
            }
        }
    }

    /// Return to continuous autofocus and clear the focus reticle.
    func resetFocus() {
        focusState = .continuous
        capture.resetFocus()
    }

    /// Toggle or set white balance lock.
    func setWhiteBalance(_ state: WhiteBalanceState) {
        whiteBalanceState = state
        state == .locked ? capture.lockWhiteBalance() : capture.unlockWhiteBalance()
    }

    /// Toggle front/back camera. Triggers a pipeline restart.
    func flipCamera() {
        settings.cameraSide = settings.cameraSide == .back ? .front : .back
        reapplySettings()
    }

    // MARK: - Settings apply

    func reapplySettings() {
        // Reset camera-control state — new device starts fresh.
        focusState = .continuous
        whiteBalanceState = .auto

        // Restart encoder + capture with new settings.
        stopEncodePipeline()
        startEncodePipeline()

        // Republish Bonjour with updated TXT.
        republishBonjour()

        // Re-register so REPORT picks up new resolution/fps.
        if let host = currentReportHost {
            doRegistration(host: host, regPort: currentRegPort)
        }
    }

    private func republishBonjour() {
        bonjourPublisher?.republish(
            name: settings.sourceName,
            resolution: resolutionString(),
            fps: settings.fps
        )
    }

    // MARK: - Helpers

    private func resolutionString() -> String {
        switch settings.resolution {
        case .vga:   return "640x480"
        case .hd720: return "1280x720"
        }
    }

    private func nativeDimensions(for r: CaptureResolution) -> (Int32, Int32) {
        switch r {
        case .vga:   return (640, 480)
        case .hd720: return (1280, 720)
        }
    }

    private static func ssrcFromName(_ name: String) -> UInt32 {
        var hash: UInt64 = 1469598103934665603
        let prime: UInt64 = 1099511628211
        for byte in name.utf8 {
            hash ^= UInt64(byte)
            hash = hash &* prime
        }
        return UInt32(truncatingIfNeeded: hash ^ (hash >> 32))
    }

    static func wifiIP() -> String? {
        var ifaddrPtr: UnsafeMutablePointer<ifaddrs>?
        guard getifaddrs(&ifaddrPtr) == 0, let first = ifaddrPtr else { return nil }
        defer { freeifaddrs(ifaddrPtr) }
        var cur: UnsafeMutablePointer<ifaddrs>? = first
        while let p = cur {
            let ifa = p.pointee
            let flags = Int32(ifa.ifa_flags)
            if let addr = ifa.ifa_addr,
               addr.pointee.sa_family == sa_family_t(AF_INET),
               (flags & IFF_UP) != 0,
               (flags & IFF_RUNNING) != 0,
               (flags & IFF_LOOPBACK) == 0,
               let name = ifa.ifa_name.map({ String(cString: $0) }),
               name == "en0" {
                var buf = [CChar](repeating: 0, count: Int(INET_ADDRSTRLEN))
                let sin = addr.withMemoryRebound(to: sockaddr_in.self, capacity: 1) { $0.pointee }
                var inAddr = sin.sin_addr
                inet_ntop(AF_INET, &inAddr, &buf, socklen_t(INET_ADDRSTRLEN))
                return String(cString: buf)
            }
            cur = ifa.ifa_next
        }
        return nil
    }
}

final class HotState: @unchecked Sendable {
    var encoder: H264Encoder?
    var packetizer: RtpPacketizer?
    var sender: RtpSender?
}
