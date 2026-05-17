import SwiftUI

struct StageStatusView: View {
    @EnvironmentObject var model: AppModel

    var body: some View {
        ZStack {
            Color.black.ignoresSafeArea()

            VStack(alignment: .leading, spacing: 0) {
                Spacer()

                Text(model.settings.sourceName)
                    .font(.title2.bold().monospaced())
                    .foregroundStyle(.white)
                    .padding(.bottom, 20)

                Divider().background(.white.opacity(0.2)).padding(.bottom, 16)

                row("STATUS",  stateLabel, stateColor)
                row("REPORT",  model.connectedReportName.isEmpty ? "—" : model.connectedReportName, .white)
                row("PORT",    model.connectedPort > 0 ? "\(model.connectedPort)" : "—", .white)
                row("LOCAL",   model.localIP, .white)

                Divider().background(.white.opacity(0.2)).padding(.vertical, 16)

                row("STREAM",  "\(model.settings.resolution.label) / \(model.settings.fps) fps", .white)
                row("BITRATE", "\(model.settings.bitrateKbps) kbps", .white)
                row("ZOOM",    String(format: "%.1f×", model.settings.zoomFactor), .white)
                row("EV",      String(format: "%+.1f", model.settings.exposureBias), .white)

                Divider().background(.white.opacity(0.2)).padding(.vertical, 16)

                row("PACKETS", formatted(model.sentPackets), .white)
                row("SENT",    formattedBytes(model.sentBytes), .white)

                Spacer()

                HStack {
                    Spacer()
                    Text("double-tap to exit stage mode")
                        .font(.caption2.monospaced())
                        .foregroundStyle(.white.opacity(0.25))
                    Spacer()
                }
                .padding(.bottom, 30)
            }
            .padding(.horizontal, 40)
        }
        .contentShape(Rectangle())
        .onTapGesture(count: 2) {
            model.settings.stageMode = false
        }
    }

    private var stateLabel: String {
        switch model.connectionState {
        case .searching:   return "SEARCHING"
        case .registering: return "REGISTERING"
        case .streaming:   return "STREAMING"
        case .live:        return "LIVE ●"
        case .lost:        return "LOST ⚠"
        }
    }

    private var stateColor: Color {
        switch model.connectionState {
        case .searching, .registering: return .gray
        case .streaming:               return .orange
        case .live:                    return .green
        case .lost:                    return .yellow
        }
    }

    @ViewBuilder
    private func row(_ label: String, _ value: String, _ valueColor: Color) -> some View {
        HStack(alignment: .firstTextBaseline) {
            Text(label)
                .font(.caption.monospaced())
                .foregroundStyle(.white.opacity(0.45))
                .frame(width: 72, alignment: .leading)
            Text(value)
                .font(.body.monospaced())
                .foregroundStyle(valueColor)
                .lineLimit(1)
        }
        .padding(.bottom, 8)
    }

    private func formatted(_ n: UInt64) -> String {
        let fmt = NumberFormatter()
        fmt.numberStyle = .decimal
        return fmt.string(from: NSNumber(value: n)) ?? "\(n)"
    }

    private func formattedBytes(_ b: UInt64) -> String {
        let mb = Double(b) / 1_000_000
        if mb < 1000 { return String(format: "%.1f MB", mb) }
        return String(format: "%.2f GB", mb / 1000)
    }
}
