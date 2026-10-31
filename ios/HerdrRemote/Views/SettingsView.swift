import ActivityKit
import HerdrRemoteCore
import SwiftUI

/// Settings A: one grouped list.
struct SettingsView: View {
    @Environment(AppModel.self) private var model
    @State private var cacheSize = 0
    @AppStorage(AppLock.setting, store: AppLock.store) private var requireFaceID = false

    var body: some View {
        List {
            Section("Computers") {
                ForEach(model.computers) { computer in
                    VStack(alignment: .leading, spacing: 2) {
                        Text(model.displayName(computer))
                        if let vouching = model.vouching(computer) {
                            Text("via \(model.displayName(vouching))").font(.caption).foregroundStyle(.secondary)
                        }
                        Text(computer.origins.joined(separator: " · ")).font(.caption).foregroundStyle(.secondary)
                        Text(fingerprintLine(computer)).font(.caption2.monospaced()).foregroundStyle(.secondary)
                    }
                    .swipeActions {
                        Button("Forget", role: .destructive) { model.forget(computer) }
                    }
                }
            }
            if !model.computers.isEmpty {
                Section {
                    ForEach(model.computers) { computer in
                        Toggle(model.displayName(computer), isOn: Binding(
                            get: { computer.presence },
                            set: { model.setPresence($0, for: computer) }
                        ))
                    }
                } header: {
                    Text("Hold alerts at the desk")
                } footer: {
                    Text("Over Bluetooth, this phone tells the computer when you are near it, and the computer holds phone alerts while you are there and using it. It also keeps the computer's addresses current. The computer needs presence on in Herdr's Remote control settings.")
                }
            }
            Section("Chats") {
                NavigationLink {
                    TextShortcutsView()
                } label: {
                    LabeledContent("Text Shortcuts", value: "\(model.shortcuts.shortcuts.count)")
                }
            }
            Section {
                if !ActivityAuthorizationInfo().areActivitiesEnabled {
                    Label("Live Activities are off for Herdr Remote. Turn them on in Settings.", systemImage: "exclamationmark.triangle")
                }
                #if DEBUG
                Button("Start a sample activity") { model.startSample() }
                if let status = model.sampleStatus {
                    Text(status).font(.footnote).foregroundStyle(.secondary)
                }
                #endif
            } header: {
                Text("Live Activities")
            }
            Section {
                Toggle("Require Face ID", isOn: Binding(
                    get: { requireFaceID },
                    set: { on in
                        // Turning the lock off asks too, so a borrowed,
                        // unlocked phone cannot drop it.
                        Task {
                            if await OwnerCheck.confirm(on ? "Lock Herdr Remote" : "Stop locking Herdr Remote") {
                                requireFaceID = on
                                model.appLockChanged()
                            }
                        }
                    }
                ))
            } header: {
                Text("Privacy")
            } footer: {
                Text("Asks for Face ID when the app opens and hides it in the app switcher. \"Always\" answers and auto-accept ask either way.")
            }
            Section("Storage") {
                LabeledContent("Cached chats and lists", value: ByteCountFormatter.string(fromByteCount: Int64(cacheSize), countStyle: .file))
                Button("Clear cache", role: .destructive) {
                    model.fleet.cache.clear()
                    cacheSize = 0
                }
            }
            if let problem = model.tokens.lastError ?? model.message {
                Section("Last problem") { Text(problem).font(.footnote).foregroundStyle(.secondary) }
            }
            Section {
                LabeledContent("Version", value: Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "")
            }
        }
        .navigationBarTitleDisplayMode(.inline)
        .task { cacheSize = model.fleet.cache.size() }
    }

    /// The fingerprint in groups of four, the way the desktop shows it.
    private func fingerprintLine(_ computer: Computer) -> String {
        let hex = Array(computer.fingerprint.hex.prefix(16))
        return stride(from: 0, to: hex.count, by: 4).map { String(hex[$0..<min($0 + 4, hex.count)]) }.joined(separator: " ")
    }
}
