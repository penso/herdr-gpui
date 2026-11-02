import SwiftUI
import UIKit

/// Pairing: pick a computer nearby or scan the QR Herdr shows, compare the
/// matching code with the computer's dialog, and wait for Allow.
struct PairView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var pairing = PairingModel()
    @State private var deviceName = UIDevice.current.name

    var body: some View {
        NavigationStack {
            Group {
                switch pairing.step {
                case .idle:
                    scan
                case let .working(text):
                    ProgressView(text)
                case let .code(code):
                    codeView(code)
                case let .paired(name):
                    // The sheet closes on its own unless the computer pairs
                    // the phone with its hosts too; the new computer is
                    // already on the home screen behind it.
                    if let computer = pairing.paired {
                        VStack(alignment: .leading, spacing: 20) {
                            Label("Paired with \(name)", systemImage: "checkmark.circle.fill")
                                .font(.title3.bold()).foregroundStyle(.green)
                            AlsoPairing(computer: computer) { dismiss() }
                            Spacer()
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding()
                    }
                case let .failed(message):
                    ContentUnavailableView {
                        Label("Pairing failed", systemImage: "xmark.octagon")
                    } description: {
                        Text(message)
                    } actions: {
                        Button("Try again") { pairing.cancel() }
                    }
                }
            }
            .sensoryFeedback(trigger: pairing.step) { _, step in
                if case .paired = step { .success } else { nil }
            }
            .navigationTitle("Pair a computer")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button(pairing.paired == nil ? "Cancel" : "Done") {
                        pairing.cancel()
                        dismiss()
                    }
                }
            }
        }
    }

    private var scan: some View {
        Form {
            // Computers already paired stay listed: the computer may have
            // removed this phone, and pairing again replaces the record.
            let nearby = (model.nearby.computers + model.bluetooth.computers).filter(\.pairing)
            Section {
                ForEach(nearby) { computer in
                    Button {
                        pairing.pair(nearby: computer, bluetooth: model.bluetooth, deviceName: deviceName) { model.add($0) }
                    } label: {
                        HStack {
                            Label(computer.name, systemImage: "laptopcomputer")
                            Spacer()
                            if let machine = model.fleet.machines.first(where: { computer.matches($0.computer.fingerprint) }) {
                                if case .revoked = machine.status {
                                    Text("Revoked").font(.footnote).foregroundStyle(.red)
                                } else {
                                    Text("Paired").font(.footnote).foregroundStyle(.secondary)
                                }
                            }
                            if case .bluetooth = computer.route {
                                Image(systemName: "dot.radiowaves.left.and.right").accessibilityLabel("Bluetooth")
                            } else {
                                Image(systemName: "wifi").accessibilityLabel("Same network")
                            }
                        }
                    }
                }
                if nearby.isEmpty {
                    HStack(spacing: 8) {
                        ProgressView().controlSize(.small)
                        Text("Looking for computers nearby").foregroundStyle(.secondary)
                    }
                }
            } header: {
                Text("Nearby")
            } footer: {
                Text("A computer appears here while Remote control > Pair device is open on it.")
            }
            Section {
                QRScanner { text in start(text) }
                    .frame(height: 280)
                    .listRowInsets(EdgeInsets())
            } footer: {
                Text("In Herdr, open Remote control > Pair device and point the camera at the QR.")
            }
            Section("This phone's name on the computer") {
                TextField("iPhone", text: $deviceName)
            }
        }
    }

    private func codeView(_ code: String) -> some View {
        VStack(spacing: 20) {
            Text("Check that the computer shows the same code, then choose Allow there.")
                .multilineTextAlignment(.center)
                .foregroundStyle(.secondary)
            Text(spaced(code))
                .font(.system(size: 44, weight: .semibold, design: .monospaced))
                .accessibilityLabel(code.map(String.init).joined(separator: " "))
            ProgressView("Waiting for the computer…")
        }
        .padding()
    }

    /// `428913` as `428 913`, as both screens show it.
    private func spaced(_ code: String) -> String {
        guard code.count == 6 else { return code }
        return "\(code.prefix(3)) \(code.suffix(3))"
    }

    private func start(_ text: String) {
        let name = deviceName.trimmingCharacters(in: .whitespacesAndNewlines)
        pairing.pair(text, deviceName: name.isEmpty ? "iPhone" : String(name.prefix(64))) { computer in
            model.add(computer)
        }
    }
}
