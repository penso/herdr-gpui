import HerdrRemoteCore
import SwiftUI
import UIKit

/// Pairing B: a card that rises when a computer nearby has its pairing
/// dialog open, and turns into the code to compare.
struct ProximityCard: View {
    @Environment(AppModel.self) private var model
    let computer: NearbyComputer
    let onClose: () -> Void
    @State private var pairing = PairingModel()

    var body: some View {
        VStack(spacing: 16) {
            HStack {
                Spacer()
                Button(action: close) {
                    Image(systemName: "xmark").font(.body.weight(.bold)).frame(width: 30, height: 30)
                }
                .buttonStyle(.glass)
                .buttonBorderShape(.circle)
                .accessibilityLabel("Close")
            }
            Image(systemName: "laptopcomputer").font(.system(size: 72, weight: .thin)).foregroundStyle(.tint)
            switch pairing.step {
            case .idle:
                Text(computer.name).font(.title2.bold())
                Text("Herdr is ready to pair. Its chats, terminals, and notifications will come to this \(UIDevice.current.model).")
                    .font(.subheadline).foregroundStyle(.secondary).multilineTextAlignment(.center)
                Label(via, systemImage: viaSymbol).font(.caption.weight(.semibold))
                    .padding(.horizontal, 10).padding(.vertical, 4).background(.fill.tertiary, in: .capsule)
                Button { start() } label: { Text("Pair").font(.headline).frame(maxWidth: .infinity) }
                    .buttonStyle(.glassProminent).controlSize(.extraLarge)
                Button("Not now", action: close).font(.subheadline)
            case let .working(message):
                ProgressView(message).padding(.vertical, 24)
            case let .code(code):
                Text("Is this your code?").font(.title2.bold())
                Text(spaced(code))
                    .font(.system(size: 54, weight: .semibold, design: .rounded).monospacedDigit())
                    .accessibilityLabel(code.map(String.init).joined(separator: " "))
                Text("\(computer.name) shows a code too. Choose **Allow** there if they match.")
                    .font(.subheadline).foregroundStyle(.secondary).multilineTextAlignment(.center)
                Button(role: .destructive, action: close) { Text("They don't match").frame(maxWidth: .infinity) }
                    .buttonStyle(.glass).controlSize(.large)
            case let .paired(name):
                Label("Paired with \(name)", systemImage: "checkmark.circle.fill").font(.title3.bold()).foregroundStyle(.green)
                // The card closes on its own unless the computer pairs the
                // phone with its hosts too; then the close button does.
                if let paired = pairing.paired {
                    AlsoPairing(computer: paired) {
                        Task {
                            try? await Task.sleep(for: .seconds(1.2))
                            onClose()
                        }
                    }
                }
            case let .failed(message):
                Text(message).multilineTextAlignment(.center)
                Button("Close", action: close).buttonStyle(.glass)
            }
        }
        .padding(24)
        .background(Color(.secondarySystemBackground), in: .rect(cornerRadius: 44))
        .padding(10)
        .sensoryFeedback(.success, trigger: pairing.step) { _, step in if case .code = step { true } else { false } }
        .transition(.move(edge: .bottom).combined(with: .opacity))
        #if DEBUG
        // `-nearbyPair YES` presses Pair, for the simulator loop.
        .task { if UserDefaults.standard.bool(forKey: "nearbyPair") { start() } }
        #endif
    }

    private var via: String {
        if case .bluetooth = computer.route { "Bluetooth" } else { "Same network" }
    }

    private var viaSymbol: String {
        if case .bluetooth = computer.route { "dot.radiowaves.left.and.right" } else { "wifi" }
    }

    private func start() {
        pairing.pair(nearby: computer, bluetooth: model.bluetooth, deviceName: UIDevice.current.name) { model.add($0) }
    }

    private func close() {
        pairing.cancel()
        onClose()
    }

    private func spaced(_ code: String) -> String {
        code.count == 6 ? "\(code.prefix(3)) \(code.suffix(3))" : code
    }
}
