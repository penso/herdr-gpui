import HerdrRemoteCore
import SwiftUI

/// Connection C: a capsule above the tab bar that says which computers
/// are not live. Connecting shows only after a moment, since the link is
/// usually back before anyone could read it; it slides away once live.
/// An offline or unpaired computer keeps it, with what to do next.
struct ConnectionToast: View {
    @Environment(AppModel.self) private var model
    @Binding var pairing: Bool
    /// Whether connecting has lasted long enough to mention.
    @State private var patient = false

    /// How long a link may connect before the toast says so.
    static let grace: Duration = .seconds(1.5)

    private enum Problem: Equatable {
        case connecting([String])
        case offline(names: [String], since: Date?, reason: String)
        case revoked([String])
        case outdated([String])
    }

    var body: some View {
        let machines = model.fleet.machines
        let connecting = machines.filter { $0.status == .connecting }.map(\.computer.name)
        let problem = problem(machines, connecting: connecting)
        Group {
            if let problem {
                toast(problem)
                    .transition(.move(edge: .bottom).combined(with: .opacity))
            }
        }
        .animation(.spring(duration: 0.35), value: problem)
        .task(id: connecting) {
            patient = false
            guard !connecting.isEmpty else { return }
            try? await Task.sleep(for: Self.grace)
            patient = !Task.isCancelled
        }
    }

    /// The worst state wins: unpaired, then outdated, offline, connecting.
    private func problem(_ machines: [Fleet.Machine], connecting: [String]) -> Problem? {
        let revoked = machines.filter { $0.status == .revoked }.map(\.computer.name)
        if !revoked.isEmpty { return .revoked(revoked) }
        let outdated = machines.filter { $0.status == .outdated }.map(\.computer.name)
        if !outdated.isEmpty { return .outdated(outdated) }
        let offline = machines.filter { if case .offline = $0.status { true } else { false } }
        if let first = offline.first {
            let reason = if case let .offline(reason) = first.status { reason } else { "" }
            // The oldest view still shown is the one worth dating.
            let since = offline.compactMap(\.updated).min()
            return .offline(names: offline.map(\.computer.name), since: since, reason: reason)
        }
        if patient, !connecting.isEmpty { return .connecting(connecting) }
        return nil
    }

    @ViewBuilder
    private func toast(_ problem: Problem) -> some View {
        switch problem {
        case let .connecting(names):
            capsule {
                ProgressView().controlSize(.small)
                Text("Connecting to \(Self.list(names))").font(.subheadline.weight(.medium)).lineLimit(1)
            }
        case let .offline(names, since, reason):
            capsule(wide: true) {
                Image(systemName: "wifi.slash").foregroundStyle(.orange)
                VStack(alignment: .leading, spacing: 0) {
                    Text(names.count == 1 ? "\(names[0]) is offline" : "\(names.count) computers are offline")
                        .font(.subheadline.weight(.medium)).lineLimit(1)
                    Text(since.map { "Showing chats as of \(Self.time($0))" } ?? reason)
                        .font(.caption).foregroundStyle(.secondary).lineLimit(1)
                }
                Spacer(minLength: 8)
                Button("Retry") { model.fleet.networkChanged() }
                    .buttonStyle(.glass).controlSize(.small)
            }
        case let .outdated(names):
            capsule(wide: true) {
                Image(systemName: "arrow.up.circle").foregroundStyle(.orange)
                VStack(alignment: .leading, spacing: 0) {
                    Text(names.count == 1 ? "\(names[0]) needs a newer Herdr" : "\(names.count) computers need a newer Herdr")
                        .font(.subheadline.weight(.medium)).lineLimit(1)
                    Text("Update Herdr there to see its chats").font(.caption).foregroundStyle(.secondary).lineLimit(1)
                }
                Spacer(minLength: 8)
                Button("Retry") { model.fleet.networkChanged() }
                    .buttonStyle(.glass).controlSize(.small)
            }
        case let .revoked(names):
            capsule(wide: true) {
                Image(systemName: "laptopcomputer.slash").foregroundStyle(.red)
                VStack(alignment: .leading, spacing: 0) {
                    Text("\(Self.list(names)) unpaired this phone").font(.subheadline.weight(.medium)).lineLimit(1)
                    Text("Pair again to see its chats").font(.caption).foregroundStyle(.secondary).lineLimit(1)
                }
                Spacer(minLength: 8)
                Button("Pair") { pairing = true }
                    .buttonStyle(.glass).controlSize(.small)
            }
        }
    }

    private func capsule(wide: Bool = false, @ViewBuilder content: () -> some View) -> some View {
        HStack(spacing: 10, content: content)
            .padding(.horizontal, 16).padding(.vertical, 10)
            .frame(maxWidth: wide ? .infinity : nil)
            .glassEffect(.regular, in: .capsule)
            .padding(.horizontal, 16).padding(.bottom, 8)
    }

    static func list(_ names: [String]) -> String {
        names.count == 1 ? names[0] : "\(names.count) computers"
    }

    /// The time alone today, the day and time before.
    static func time(_ date: Date) -> String {
        Calendar.current.isDateInToday(date)
            ? date.formatted(date: .omitted, time: .shortened)
            : date.formatted(.dateTime.month(.abbreviated).day().hour().minute())
    }
}

extension View {
    /// Says at the bottom which computers are not live, on this view and
    /// on every screen a navigation stack it wraps pushes.
    func connectionToast(pairing: Binding<Bool>) -> some View {
        safeAreaInset(edge: .bottom) { ConnectionToast(pairing: pairing) }
    }
}
