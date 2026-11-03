import HerdrRemoteCore
import SwiftUI

/// One paired computer's page: where the phone reaches it, its alerts and
/// certificate, and pairing again or forgetting it. Settings' rows and the
/// Workspaces header menu open it.
struct ComputerScreen: View {
    @Environment(AppModel.self) private var model
    @Environment(WindowState.self) private var window: WindowState?
    @Environment(\.dismiss) private var dismiss
    let device: String
    @State private var forgetting: Computer?

    var body: some View {
        if let computer = model.computers.first(where: { $0.device == device }) {
            page(computer)
        } else {
            ContentUnavailableView("Forgotten", systemImage: "laptopcomputer.slash",
                                   description: Text("This computer is no longer paired with this phone."))
        }
    }

    private func page(_ computer: Computer) -> some View {
        let name = model.displayName(computer)
        let status = model.fleet.machines.first { $0.id == device }?.status
        let addresses = Self.addresses(computer)
        let connected: String? = if case let .online(origin) = status, addresses.contains(where: { $0.origin == origin }) { origin } else { nil }
        return List {
            Section {
                VStack(spacing: 6) {
                    Image(systemName: computer.via == nil ? "laptopcomputer" : "server.rack")
                        .font(.system(size: 44, weight: .thin)).foregroundStyle(.tint)
                    Text(name).font(.title2.bold())
                    if let status { ComputerStatusLine(status: status).font(.subheadline) }
                }
                .frame(maxWidth: .infinity)
                .listRowBackground(Color.clear)
            }
            Section {
                ForEach(addresses, id: \.origin) { address in
                    LabeledContent {
                        Text(address.host).textSelection(.enabled)
                    } label: {
                        if address.origin == connected {
                            Label(address.kind.label, systemImage: "checkmark.circle.fill")
                                .labelStyle(.titleAndIcon).foregroundStyle(.green)
                                .accessibilityLabel("\(address.kind.label), connected")
                        } else {
                            Text(address.kind.label)
                        }
                    }
                }
            } header: {
                Text("Addresses")
            } footer: {
                if connected != nil {
                    Text("Connected through the checked address. When it stops answering, this phone tries the others.")
                }
            }
            if let vouching = model.vouching(computer) {
                Section {
                    LabeledContent("Paired through", value: model.displayName(vouching))
                }
            }
            Section {
                Toggle("Hold alerts at the desk", isOn: Binding(
                    get: { computer.presence },
                    set: { model.setPresence($0, for: computer) }
                ))
            } footer: {
                Text("Over Bluetooth, \(name) knows when you are near it and holds phone alerts while you use it.")
            }
            Section("Certificate") {
                Text(Self.grouped(computer.fingerprint.hex))
                    .font(.caption.monospaced()).foregroundStyle(.secondary).textSelection(.enabled)
            }
            Section {
                if status == .revoked {
                    Button("Pair Again") { window?.pairing = true }
                }
                Button("Forget This Computer", role: .destructive) { forgetting = computer }
            } footer: {
                Text(ForgetCopy.consequence(name))
            }
        }
        .navigationTitle(name)
        .navigationBarTitleDisplayMode(.inline)
        .forgetConfirmation($forgetting) { dismiss() }
    }

    /// The addresses a phone can use, labelled; loopback ones are the
    /// computer's own and are left out.
    private static func addresses(_ computer: Computer) -> [(origin: String, kind: OriginKind, host: String)] {
        computer.origins.compactMap { origin in
            let kind = OriginKind(origin: origin)
            guard kind != .loopback, let url = URL(string: origin), let host = url.host(percentEncoded: false) else { return nil }
            return (origin, kind, host)
        }
    }

    /// The fingerprint in groups of four, the way the desktop shows it.
    static func grouped(_ hex: String) -> String {
        let characters = Array(hex)
        return stride(from: 0, to: characters.count, by: 4)
            .map { String(characters[$0..<min($0 + 4, characters.count)]) }
            .joined(separator: " ")
    }
}

/// A computer's link state in words, for its page and Settings' rows.
struct ComputerStatusLine: View {
    var status: LinkStatus

    var body: some View {
        switch status {
        case .online: Label("Connected", systemImage: "circle.fill").foregroundStyle(.green)
        case .connecting: Label("Connecting…", systemImage: "circle.dotted").foregroundStyle(.secondary)
        case .offline: Label("Offline", systemImage: "wifi.slash").foregroundStyle(.secondary)
        case .revoked: Label("Unpaired this \(UIDevice.current.model)", systemImage: "exclamationmark.triangle.fill").foregroundStyle(.red)
        case .outdated: Label("Needs a newer Herdr", systemImage: "arrow.up.circle").foregroundStyle(.orange)
        }
    }
}
