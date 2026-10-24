import SwiftUI
import UIKit

/// Pairing: scan the QR Herdr shows (or paste its link), compare the
/// matching code with the computer's dialog, and wait for Allow.
struct PairView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var pairing = PairingModel()
    @State private var pasted = ""
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
                    ContentUnavailableView {
                        Label("Paired with \(name)", systemImage: "checkmark.circle.fill")
                    } description: {
                        Text("Running chats on \(name) now appear on the lock screen.")
                    } actions: {
                        Button("Done") { dismiss() }.buttonStyle(.borderedProminent)
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
            .navigationTitle("Pair a computer")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") {
                        pairing.cancel()
                        dismiss()
                    }
                }
            }
        }
    }

    private var scan: some View {
        Form {
            Section {
                QRScanner { text in start(text) }
                    .frame(height: 280)
                    .listRowInsets(EdgeInsets())
            } footer: {
                Text("In Herdr, open Remote control > Pair device and point the camera at the QR.")
            }
            Section("Or paste the pairing link") {
                TextField("https://…/pair#t=…", text: $pasted, axis: .vertical)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    .keyboardType(.URL)
                Button("Paste") {
                    pasted = UIPasteboard.general.string ?? pasted
                }
                Button("Pair") { start(pasted) }
                    .disabled(pasted.isEmpty)
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
