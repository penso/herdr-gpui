import HerdrRemoteCore
import SwiftUI

/// The hosts of one computer the phone is pairing or may pair, under that
/// computer in the Computers tab.
struct HostRows: View {
    @Environment(AppModel.self) private var model
    let computer: Computer

    var body: some View {
        ForEach(model.hostLinks.rows(computer)) { host in
            HStack {
                // Listed as paired only when no computer here has its key.
                HostLine(host: host, detail: host.state.isPaired ? "Not on this phone" : nil)
                Spacer()
                if let action = action(host) {
                    let posting = model.hostLinks.posting.contains(.init(computer: computer.fingerprint, host: host.host))
                    Button(action) { model.hostLinks.pair(host, through: computer) }
                        .buttonStyle(.bordered)
                        .controlSize(.small)
                        .disabled(posting)
                }
            }
        }
    }

    private func action(_ host: HostLink) -> String? {
        let waiting = model.hostLinks.pairings[computer.fingerprint]?.pending[host.host] != nil
        guard !waiting else { return nil }
        switch host.state {
        case .offered, .paired: return "Pair"
        case .failed: return "Retry"
        case .allowed, .asking, .pending: return nil
        }
    }
}

/// A host's name and where the phone stands with it.
struct HostLine: View {
    let host: HostLink
    var detail: String?

    var body: some View {
        Label {
            VStack(alignment: .leading, spacing: 2) {
                Text(host.name).lineLimit(1)
                Text(detail ?? host.state.summary).font(.caption).foregroundStyle(color).lineLimit(2)
            }
        } icon: {
            Image(systemName: "server.rack")
        }
    }

    private var color: Color {
        if case .failed = host.state { .red } else { .secondary }
    }
}

/// After pairing a computer: the hosts it pairs the phone with too, each
/// with its state. Calls `onNone` once the computer answered and had no
/// host to pair.
struct AlsoPairing: View {
    @Environment(AppModel.self) private var model
    let computer: Computer
    let onNone: () -> Void

    var body: some View {
        let hosts = model.hostLinks.progress(computer)
        let checking = model.hostLinks.checking.contains(computer.fingerprint)
        VStack(alignment: .leading, spacing: 12) {
            if checking, hosts.isEmpty {
                HStack(spacing: 8) {
                    ProgressView().controlSize(.small)
                    Text("Looking for \(computer.name)'s hosts").foregroundStyle(.secondary)
                }
            } else if !hosts.isEmpty {
                Text("Also pairing \(hosts.map(\.name).formatted(.list(type: .and)))")
                    .font(.headline)
                ForEach(hosts) { HostLine(host: $0) }
            }
        }
        .onChange(of: !checking && hosts.isEmpty, initial: true) { _, none in
            if none { onNone() }
        }
    }
}

extension HostState {
    /// One line for where the phone stands with a host.
    var summary: String {
        switch self {
        case .offered: "Not allowed for this phone yet; pairing asks the computer"
        case .allowed: "Allowed; pairing next"
        case .asking: "Waiting for Allow on the computer"
        case .pending: "Pairing…"
        case .paired: "Paired"
        case let .failed(reason): reason
        }
    }

    var isPaired: Bool {
        if case .paired = self { true } else { false }
    }
}
