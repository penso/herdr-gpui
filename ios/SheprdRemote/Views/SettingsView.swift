import ActivityKit
import SheprdRemoteCore
import SwiftUI
import UIKit

/// Settings A: one grouped list.
struct SettingsView: View {
    @Environment(AppModel.self) private var model
    @Environment(WindowState.self) private var window: WindowState?
    @State private var cacheSize = 0
    @State private var forgetting: Computer?
    @State private var editMode = EditMode.inactive
    @AppStorage(AppLock.setting, store: AppLock.store) private var requireFaceID = false
    @AppStorage(AppAppearance.key) private var appearance = AppAppearance.system
    @AppStorage(ActivityTokens.genericSetting) private var genericActivities = false

    var body: some View {
        List {
            Section {
                ForEach(model.computers) { computer in
                    NavigationLink {
                        ComputerScreen(device: computer.device)
                    } label: {
                        ComputerRow(computer: computer)
                    }
                    .swipeActions {
                        // Not `.destructive`: that removes the row before
                        // the confirmation answers.
                        Button("Forget", systemImage: "trash") { forgetting = computer }.tint(.red)
                    }
                }
                .onDelete { offsets in forgetting = offsets.first.map { model.computers[$0] } }
                Button("Pair a Computer", systemImage: "plus") { window?.pairing = true }
            } header: {
                HStack {
                    Text("Computers")
                    Spacer()
                    if !model.computers.isEmpty { EditButton().textCase(nil).font(.subheadline) }
                }
            } footer: {
                if !model.computers.isEmpty {
                    Text("Forgetting a computer removes its chats from this \(UIDevice.current.model). It does not stop Sheprd there.")
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
                    Text("Over Bluetooth, this phone tells the computer when you are near it, and the computer holds phone alerts while you are there and using it. It also keeps the computer's addresses current. The computer needs presence on in Sheprd's Remote control settings.")
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
                    Label("Live Activities are off for Sheprd. Turn them on in Settings.", systemImage: "exclamationmark.triangle")
                }
                Toggle("Keep Generic", isOn: $genericActivities)
                    .onChange(of: genericActivities) { _, generic in
                        Task { await model.tokens.genericChanged(to: generic) }
                    }
                #if DEBUG
                Button("Start a sample activity") { model.startSample() }
                if let status = model.sampleStatus {
                    Text(status).font(.footnote).foregroundStyle(.secondary)
                }
                #endif
            } header: {
                Text("Live Activities")
            } footer: {
                Text("Generic shows the agent and what it is doing, without the chat's name, steps, or questions. A computer can also choose this in Sheprd's Remote control settings.")
            }
            Section {
                Picker("Appearance", selection: $appearance) {
                    ForEach(AppAppearance.allCases) { Text($0.title).tag($0) }
                }
                .pickerStyle(.segmented)
                .labelsHidden()
            } header: {
                Text("Appearance")
            } footer: {
                Text("Terminals stay dark either way.")
            }
            Section {
                Toggle("Require Face ID", isOn: Binding(
                    get: { requireFaceID },
                    set: { on in
                        // Turning the lock off asks too, so a borrowed,
                        // unlocked phone cannot drop it.
                        Task {
                            if await OwnerCheck.confirm(on ? "Lock Sheprd" : "Stop locking Sheprd") {
                                requireFaceID = on
                                model.appLockChanged()
                            }
                        }
                    }
                ))
            } header: {
                Text("Privacy")
            } footer: {
                Text("Asks for Face ID when the app opens and hides it in the app switcher. \"Always\" answers ask either way.")
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
        .forgetConfirmation($forgetting)
        .environment(\.editMode, $editMode)
        #if DEBUG
        .task { if UserDefaults.standard.bool(forKey: "editComputers") { editMode = .active } }
        #endif
    }
}

/// A computer in Settings: its name, then how it stands or where it is.
private struct ComputerRow: View {
    @Environment(AppModel.self) private var model
    var computer: Computer

    var body: some View {
        let machine = model.fleet.machines.first { $0.id == computer.device }
        let status = machine?.status
        VStack(alignment: .leading, spacing: 2) {
            Text(model.displayName(computer))
            if status == .revoked {
                ComputerStatusLine(status: .revoked).font(.caption)
            } else if status == .outdated || machine?.needsUpdate == true {
                ComputerStatusLine(status: .outdated).font(.caption)
            } else if let vouching = model.vouching(computer) {
                Text("via \(model.displayName(vouching))").font(.caption).foregroundStyle(.secondary)
            } else if let kind = computer.origins.lazy.map(OriginKind.init(origin:)).first(where: { $0 != .loopback }) {
                Text(kind.label).font(.caption).foregroundStyle(.secondary)
            }
        }
    }
}
