import SwiftUI

struct ContentView: View {
    @EnvironmentObject var model: AppModel
    @State private var showSettings = false
    /// View-space point where the focus reticle should be drawn.
    @State private var reticleViewPoint: CGPoint = .zero

    var body: some View {
        ZStack {
            Color.black.ignoresSafeArea()

            if model.permissionGranted {
                CameraPreview(
                    session: model.capture.session,
                    onFocusTap: model.settings.stageMode ? nil : { devicePt, viewPt in
                        reticleViewPoint = viewPt
                        model.setFocusPoint(devicePt)
                    }
                )
                .ignoresSafeArea()
            } else {
                VStack(spacing: 12) {
                    Text("PRECOG-Cam").font(.title.monospaced())
                    Text("Camera access required.").font(.callout).foregroundStyle(.secondary)
                }
                .foregroundStyle(.white)
            }

            if model.settings.stageMode {
                StageStatusView()
            } else {
                // Focus reticle overlay
                if case let .focusing(pt) = model.focusState {
                    FocusReticle(point: reticleViewPoint, state: model.focusState)
                        .id("focusing-\(pt.x)-\(pt.y)")
                }
                if case let .locked(pt) = model.focusState {
                    FocusReticle(point: reticleViewPoint, state: model.focusState)
                        .id("locked-\(pt.x)-\(pt.y)")
                }

                // Top bar
                VStack {
                    HStack {
                        StatusPill(
                            connectionState: model.connectionState,
                            name: model.settings.sourceName,
                            connectedPort: model.connectedPort
                        )
                        Spacer()

                        // Flip camera button
                        Button { model.flipCamera() } label: {
                            Image(systemName: "camera.rotate")
                                .font(.title2)
                                .padding(10)
                                .background(.ultraThinMaterial, in: Circle())
                        }
                        .foregroundStyle(.white)

                        // Stage mode toggle
                        Button {
                            model.settings.stageMode = true
                        } label: {
                            Image(systemName: "moon.fill")
                                .font(.title2)
                                .padding(10)
                                .background(.ultraThinMaterial, in: Circle())
                        }
                        .foregroundStyle(.white)

                        Button {
                            showSettings = true
                        } label: {
                            Image(systemName: "gearshape.fill")
                                .font(.title2)
                                .padding(10)
                                .background(.ultraThinMaterial, in: Circle())
                        }
                        .foregroundStyle(.white)
                    }
                    .padding(.horizontal)
                    .padding(.top, 8)

                    Spacer()

                    if let err = model.lastError {
                        Text(err)
                            .font(.callout.monospaced())
                            .padding(8)
                            .background(.red.opacity(0.7), in: RoundedRectangle(cornerRadius: 6))
                            .foregroundStyle(.white)
                            .padding()
                    }
                }

                // Side sliders — left = exposure + WB lock, right = zoom
                let maxZoom = min(10.0, model.capture.deviceMaxZoom)
                let evMin = Double(model.capture.deviceMinExposureBias)
                let evMax = Double(model.capture.deviceMaxExposureBias)

                HStack {
                    if evMax > evMin {
                        VStack(spacing: 6) {
                            ExposureSlider(
                                value: Binding(
                                    get: { model.settings.exposureBias },
                                    set: { model.settings.exposureBias = $0 }
                                ),
                                min: evMin,
                                max: evMax
                            )
                            // White balance lock button
                            Button {
                                model.setWhiteBalance(
                                    model.whiteBalanceState == .auto ? .locked : .auto
                                )
                            } label: {
                                Text(model.whiteBalanceState == .auto ? "AWB" : "WB ■")
                                    .font(.caption.bold().monospaced())
                                    .foregroundStyle(
                                        model.whiteBalanceState == .auto ? .white : .yellow
                                    )
                                    .frame(width: 44, height: 28)
                                    .background(.ultraThinMaterial, in: Capsule())
                            }
                        }
                        .padding(.leading, 12)
                    }
                    Spacer()
                    if maxZoom > 1.0 {
                        ZoomSlider(
                            value: Binding(
                                get: { model.settings.zoomFactor },
                                set: { model.settings.zoomFactor = $0 }
                            ),
                            maxZoom: maxZoom
                        )
                        .padding(.trailing, 12)
                    }
                }
            }
        }
        .task { await model.bootstrap() }
        .sheet(isPresented: $showSettings) {
            SettingsView(settings: model.settings)
                .environmentObject(model)
        }
        .preferredColorScheme(.dark)
    }
}

// MARK: - Focus Reticle

private struct FocusReticle: View {
    let point: CGPoint
    let state: FocusState

    @State private var appeared = false
    @State private var pulse = false

    private let size: CGFloat = 60
    private let bracketLen: CGFloat = 14
    private let lineWidth: CGFloat = 2

    var body: some View {
        ZStack {
            // Corner bracket square using four L-shaped paths
            CornerBrackets(size: size, bracketLen: bracketLen, lineWidth: lineWidth)
                .stroke(.white, lineWidth: lineWidth)
        }
        .frame(width: size, height: size)
        .scaleEffect(appeared ? 1.0 : 1.3)
        .opacity(pulse ? 0.55 : 1.0)
        .position(point)
        .onAppear {
            withAnimation(.easeOut(duration: 0.2)) {
                appeared = true
            }
            if case .focusing = state {
                withAnimation(.easeInOut(duration: 0.4).repeatForever(autoreverses: true)) {
                    pulse = true
                }
            }
        }
        .onChange(of: state) { newState in
            if case .locked = newState {
                // Stop pulsing once locked
                withAnimation(.easeOut(duration: 0.2)) {
                    pulse = false
                }
            }
        }
    }
}

private struct CornerBrackets: Shape {
    let size: CGFloat
    let bracketLen: CGFloat
    let lineWidth: CGFloat

    func path(in rect: CGRect) -> Path {
        var p = Path()
        let s = bracketLen
        let w = size
        let h = size
        let ox = rect.minX
        let oy = rect.minY

        // Top-left
        p.move(to: CGPoint(x: ox, y: oy + s))
        p.addLine(to: CGPoint(x: ox, y: oy))
        p.addLine(to: CGPoint(x: ox + s, y: oy))

        // Top-right
        p.move(to: CGPoint(x: ox + w - s, y: oy))
        p.addLine(to: CGPoint(x: ox + w, y: oy))
        p.addLine(to: CGPoint(x: ox + w, y: oy + s))

        // Bottom-right
        p.move(to: CGPoint(x: ox + w, y: oy + h - s))
        p.addLine(to: CGPoint(x: ox + w, y: oy + h))
        p.addLine(to: CGPoint(x: ox + w - s, y: oy + h))

        // Bottom-left
        p.move(to: CGPoint(x: ox + s, y: oy + h))
        p.addLine(to: CGPoint(x: ox, y: oy + h))
        p.addLine(to: CGPoint(x: ox, y: oy + h - s))

        return p
    }
}

// MARK: - Sliders

private let sliderTrackHeight: CGFloat = 200

private struct ZoomSlider: View {
    @Binding var value: Double
    let maxZoom: Double

    var body: some View {
        VStack(spacing: 8) {
            Text(String(format: "%.1f×", value))
                .font(.caption.bold().monospaced())
                .foregroundStyle(.white)
                .padding(.horizontal, 8)
                .padding(.vertical, 3)
                .background(.ultraThinMaterial, in: Capsule())

            Slider(value: $value, in: 1.0...maxZoom, step: 0.1)
                .tint(.white)
                .frame(width: sliderTrackHeight)
                .rotationEffect(.degrees(-90))
                .frame(width: 44, height: sliderTrackHeight)

            Button { value = 1.0 } label: {
                Text("1×")
                    .font(.caption.bold().monospaced())
                    .foregroundStyle(.white)
                    .frame(width: 36, height: 28)
                    .background(.ultraThinMaterial, in: Capsule())
            }
        }
        .padding(.vertical, 10)
        .padding(.horizontal, 6)
        .background(.ultraThinMaterial.opacity(0.5), in: RoundedRectangle(cornerRadius: 16))
    }
}

private struct ExposureSlider: View {
    @Binding var value: Double
    let min: Double
    let max: Double

    var body: some View {
        VStack(spacing: 8) {
            Image(systemName: "sun.max.fill")
                .font(.caption)
                .foregroundStyle(.white.opacity(0.7))

            Slider(value: $value, in: min...max, step: 0.1)
                .tint(.yellow)
                .frame(width: sliderTrackHeight)
                .rotationEffect(.degrees(-90))
                .frame(width: 44, height: sliderTrackHeight)

            Image(systemName: "sun.min.fill")
                .font(.caption)
                .foregroundStyle(.white.opacity(0.7))

            Text(String(format: "%+.1f", value))
                .font(.caption.bold().monospaced())
                .foregroundStyle(.white)
                .padding(.horizontal, 8)
                .padding(.vertical, 3)
                .background(.ultraThinMaterial, in: Capsule())

            Button { value = 0.0 } label: {
                Text("0EV")
                    .font(.caption.bold().monospaced())
                    .foregroundStyle(.white)
                    .frame(width: 36, height: 28)
                    .background(.ultraThinMaterial, in: Capsule())
            }
        }
        .padding(.vertical, 10)
        .padding(.horizontal, 6)
        .background(.ultraThinMaterial.opacity(0.5), in: RoundedRectangle(cornerRadius: 16))
    }
}

// MARK: - Status pill

private struct StatusPill: View {
    let connectionState: ConnectionState
    let name: String
    let connectedPort: UInt16

    private var dotColor: Color {
        switch connectionState {
        case .searching, .registering: return .gray
        case .streaming:               return .orange
        case .live:                    return .green
        case .lost:                    return .yellow
        }
    }

    private var dotSymbol: String {
        switch connectionState {
        case .searching:   return "arrow.triangle.2.circlepath"
        case .registering: return "circle.dotted"
        case .streaming:   return "circle.fill"
        case .live:        return "circle.fill"
        case .lost:        return "exclamationmark.triangle.fill"
        }
    }

    private var stateLabel: String {
        switch connectionState {
        case .searching:              return "SEARCHING"
        case .registering:            return "REGISTERING"
        case .streaming:              return "STREAMING"
        case .live:                   return "LIVE"
        case .lost:                   return "LOST"
        }
    }

    private var detail: String {
        switch connectionState {
        case .searching, .registering:
            return "—"
        case .streaming:
            return connectedPort > 0 ? ":\(connectedPort)" : "—"
        case .live(let reportName):
            return connectedPort > 0 ? "\(reportName) :\(connectedPort)" : reportName
        case .lost(let reportName):
            return reportName
        }
    }

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: dotSymbol)
                .font(.caption.bold())
                .foregroundStyle(dotColor)
            Text(stateLabel)
                .font(.caption.bold().monospaced())
            VStack(alignment: .leading, spacing: 0) {
                Text(name).font(.caption.monospaced()).lineLimit(1)
                Text(detail)
                    .font(.caption2.monospaced())
                    .foregroundStyle(.white.opacity(0.7))
                    .lineLimit(1)
            }
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 6)
        .background(.ultraThinMaterial, in: Capsule())
        .foregroundStyle(.white)
    }
}
