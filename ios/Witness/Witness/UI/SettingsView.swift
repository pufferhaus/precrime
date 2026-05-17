import SwiftUI

struct SettingsView: View {
    @EnvironmentObject var model: AppModel
    @ObservedObject var settings: AppSettings
    @Environment(\.dismiss) private var dismiss
    @State private var nameDraft: String = ""
    @State private var bitrateDraft: String = ""

    var body: some View {
        NavigationStack {
            Form {
                Section("Source identity") {
                    TextField("PRECOG-NN-TYPE-LOC", text: $nameDraft)
                        .textInputAutocapitalization(.characters)
                        .autocorrectionDisabled()
                        .font(.body.monospaced())
                }

                Section("REPORT connection") {
                    LabeledContent("Report") {
                        Text(model.connectedReportName.isEmpty ? "—" : model.connectedReportName)
                            .font(.body.monospaced())
                            .foregroundStyle(.secondary)
                    }
                    LabeledContent("Port") {
                        Text(model.connectedPort > 0 ? "\(model.connectedPort)" : "—")
                            .font(.body.monospaced())
                            .foregroundStyle(.secondary)
                    }
                    LabeledContent("State") {
                        Text(connectionStateLabel)
                            .font(.body.monospaced())
                            .foregroundStyle(.secondary)
                    }
                    if !settings.targetHost.isEmpty {
                        LabeledContent("Fallback") {
                            Text("\(settings.targetHost):\(settings.targetPort)")
                                .font(.caption.monospaced())
                                .foregroundStyle(.secondary)
                        }
                    }
                }

                Section("Camera") {
                    Picker("Side", selection: $settings.cameraSide) {
                        ForEach(CameraSide.allCases) { side in
                            Text(side.rawValue.capitalized).tag(side)
                        }
                    }
                    Picker("Resolution", selection: $settings.resolution) {
                        ForEach(CaptureResolution.allCases) { r in
                            Text(r.label).tag(r)
                        }
                    }
                    Picker("Frame rate", selection: $settings.fps) {
                        Text("30 fps").tag(Int32(30))
                        Text("60 fps").tag(Int32(60))
                    }
                }

                Section("Camera controls") {
                    VStack(alignment: .leading, spacing: 4) {
                        HStack {
                            Text("Zoom")
                            Spacer()
                            Text(String(format: "%.1f×", settings.zoomFactor))
                                .font(.callout.monospaced())
                                .foregroundStyle(.secondary)
                        }
                        let maxZ = min(10.0, model.capture.deviceMaxZoom)
                        if maxZ > 1.0 {
                            Slider(value: $settings.zoomFactor, in: 1.0...maxZ, step: 0.1)
                        }
                    }
                    VStack(alignment: .leading, spacing: 4) {
                        HStack {
                            Text("Exposure (EV)")
                            Spacer()
                            Text(String(format: "%+.1f", settings.exposureBias))
                                .font(.callout.monospaced())
                                .foregroundStyle(.secondary)
                        }
                        let evMin = Double(model.capture.deviceMinExposureBias)
                    let evMax = Double(model.capture.deviceMaxExposureBias)
                    if evMax > evMin {
                        Slider(value: $settings.exposureBias,
                               in: evMin...evMax,
                               step: 0.1)
                    }
                    }
                    Button {
                        settings.zoomFactor = 1.0
                        settings.exposureBias = 0.0
                    } label: {
                        Text("Reset zoom + exposure")
                            .font(.callout)
                    }
                }

                Section("Stage mode") {
                    Toggle("Enable stage mode", isOn: $settings.stageMode)
                    Text("Screen dims to near-black so the phone is invisible on stage. Camera and RTP keep running. iOS prohibits background camera access — stage mode is the workaround.\n\nDouble-tap anywhere on screen to exit.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }

                Section("Kiosk mode") {
                    Toggle("Lock to this app on launch", isOn: $settings.kioskMode)
                    Text("Requests Guided Access on startup — locks the phone to WITNESS so the home button, control centre, and all other apps are inaccessible.\n\nRequires: Settings → Accessibility → Guided Access → ON, with a passcode set.\n\nTo exit: triple-click the side button and enter the Guided Access passcode.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }

                Section("Encoder") {
                    HStack {
                        Text("Bitrate (kbps)")
                        Spacer()
                        TextField("2000", text: $bitrateDraft)
                            .keyboardType(.numberPad)
                            .multilineTextAlignment(.trailing)
                            .font(.body.monospaced())
                    }
                }

                Section {
                    Button {
                        applyDrafts()
                        model.reapplySettings()
                        dismiss()
                    } label: {
                        Text("Apply & Restart Stream")
                            .frame(maxWidth: .infinity)
                            .font(.body.bold())
                    }
                }

                Section("About") {
                    LabeledContent("Bundle", value: "art.precrime.witness")
                    LabeledContent("Transport", value: "H.264 / RTP / UDP unicast")
                    LabeledContent("RTP PT / clock", value: "96 / 90 kHz")
                    Text("Unicast today. Multicast parity with native PRECOG instances is blocked on Apple's `com.apple.developer.networking.multicast` entitlement (paid Apple Dev + manual Apple approval).")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
            .onAppear {
                nameDraft = settings.sourceName
                bitrateDraft = String(settings.bitrateKbps)
            }
            .navigationTitle("Settings")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
            }
        }
    }

    private var connectionStateLabel: String {
        switch model.connectionState {
        case .searching:   return "SEARCHING"
        case .registering: return "REGISTERING"
        case .streaming:   return "STREAMING"
        case .live:        return "LIVE"
        case .lost:        return "LOST"
        }
    }

    private func applyDrafts() {
        settings.sourceName = nameDraft
        if let b = Int32(bitrateDraft), b > 0 {
            settings.bitrateKbps = b
        }
    }
}
