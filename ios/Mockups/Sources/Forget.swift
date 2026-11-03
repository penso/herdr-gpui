import SwiftUI

// Forgetting a computer. Today it is a swipe in Settings or a long press on
// a Workspaces header, with no confirmation. A: each computer gets a page,
// with Forget at the bottom like the system's Bluetooth devices. B: a
// visible menu on each computer's header. C: an unpaired computer asks
// what to do in a card. D: Settings' computer list gets an Edit mode.

/// A computer's state for these mockups.
private struct PairedMachine: Identifiable {
    let id = UUID()
    var name: String
    var symbol: String
    var route: String
    var unpaired = false
}

private let machines = [
    PairedMachine(name: "MacBook Pro", symbol: "laptopcomputer", route: "Wi-Fi · 192.168.1.20"),
    PairedMachine(name: "studio", symbol: "macstudio", route: "Tailscale · 100.88.4.2", unpaired: true),
    PairedMachine(name: "build-01", symbol: "server.rack", route: "SSH through MacBook Pro"),
]

/// What forgetting does, said the same way everywhere.
private let forgetMessage = "Its chats and cached messages leave this iPhone. Herdr keeps running on studio; pair again any time from its Remote control settings."

private struct MachineRow: View {
    var machine: PairedMachine

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: machine.symbol).frame(width: 28)
            VStack(alignment: .leading, spacing: 1) {
                Text(machine.name)
                if machine.unpaired {
                    Label("Unpaired this iPhone", systemImage: "exclamationmark.triangle.fill")
                        .font(.caption).foregroundStyle(.red)
                } else {
                    Text(machine.route).font(.caption).foregroundStyle(.secondary)
                }
            }
        }
    }
}

// MARK: A, a page per computer

struct ForgetPage: View {
    var state: Int

    var body: some View {
        NavigationStack {
            if state == 0 { list } else { page }
        }
    }

    private var list: some View {
        List {
            Section("Computers") {
                ForEach(machines) { machine in
                    NavigationLink {} label: { MachineRow(machine: machine) }
                }
                Label("Pair a Computer", systemImage: "plus").foregroundStyle(.tint)
            }
            Section("Notifications") {
                Toggle("Questions and permissions", isOn: .constant(true))
                Toggle("Finished turns", isOn: .constant(true))
            }
        }
        .navigationTitle("Settings")
    }

    private var page: some View {
        List {
            Section {
                VStack(spacing: 8) {
                    Image(systemName: "macstudio").font(.system(size: 48, weight: .thin)).foregroundStyle(.tint)
                    Text("studio").font(.title2.bold())
                    Label("Unpaired this iPhone", systemImage: "exclamationmark.triangle.fill")
                        .font(.subheadline).foregroundStyle(.red)
                }
                .frame(maxWidth: .infinity).listRowBackground(Color.clear)
            }
            Section("Addresses") {
                LabeledContent("Tailscale", value: "100.88.4.2")
                LabeledContent("Wi-Fi", value: "192.168.1.31")
                LabeledContent("Local", value: "studio.local")
            }
            Section {
                Toggle("Hold alerts at the desk", isOn: .constant(false))
            } footer: {
                Text("Over Bluetooth, studio knows when you are near it.")
            }
            Section("Certificate") {
                Text("57C3 0876 3C9A E937 D3AD 93D1 2E09 D517").font(.caption.monospaced()).foregroundStyle(.secondary)
            }
            Section {
                Button("Pair Again") {}
                Button("Forget This Computer", role: .destructive) {}
            } footer: {
                Text(forgetMessage)
            }
        }
        .navigationTitle("studio")
        .navigationBarTitleDisplayMode(.inline)
    }
}

// MARK: B, a visible menu on each header

struct ForgetHeaderMenu: View {
    var state: Int

    var body: some View {
        NavigationStack {
            List {
                ForEach(machines.prefix(2)) { machine in
                    Section {
                        Label("herdr-gpui", systemImage: "folder")
                        Label("site", systemImage: "folder")
                    } header: {
                        HStack {
                            Image(systemName: machine.symbol)
                            Text(machine.name).font(.headline).foregroundStyle(.primary)
                            if machine.unpaired {
                                Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(.red)
                            }
                            Spacer()
                            Image(systemName: "ellipsis.circle").font(.title3).foregroundStyle(.tint)
                        }
                        .textCase(nil)
                    }
                }
            }
            .navigationTitle("Workspaces")
            .overlay(alignment: .topTrailing) {
                if state == 1 { menu.padding(.top, 330).padding(.trailing, 20) }
            }
        }
    }

    /// Drawn open: a real Menu cannot be opened from a launch argument.
    private var menu: some View {
        VStack(alignment: .leading, spacing: 0) {
            menuRow("Open in Safari", "safari")
            Divider()
            menuRow("Hold Alerts at the Desk", "bell.badge")
            Divider()
            menuRow("Pair Again", "arrow.triangle.2.circlepath")
            Divider()
            menuRow("Forget studio…", "trash", destructive: true)
        }
        .frame(width: 260)
        .background(.regularMaterial, in: .rect(cornerRadius: 18))
        .shadow(radius: 20)
    }

    private func menuRow(_ title: String, _ symbol: String, destructive: Bool = false) -> some View {
        HStack {
            Text(title)
            Spacer()
            Image(systemName: symbol)
        }
        .foregroundStyle(destructive ? .red : .primary)
        .padding(.horizontal, 16).padding(.vertical, 12)
    }
}

// MARK: C, an unpaired computer asks

struct ForgetUnpairedCard: View {
    var state: Int
    @State private var confirming = false

    var body: some View {
        NavigationStack {
            List {
                Section {
                    VStack(alignment: .leading, spacing: 10) {
                        Label("studio unpaired this iPhone", systemImage: "exclamationmark.triangle.fill")
                            .font(.headline).foregroundStyle(.red)
                        Text("Someone removed this iPhone in Herdr's Remote control settings on studio. Its chats stop updating here.")
                            .font(.subheadline).foregroundStyle(.secondary)
                        HStack {
                            Button("Forget", role: .destructive) { confirming = true }
                                .buttonStyle(.bordered).tint(.red)
                            Button("Pair Again") {}.buttonStyle(.borderedProminent)
                        }
                    }
                    .padding(.vertical, 4)
                }
                Section("Needs you") {
                    ChatRowView(chat: Sample.computers[0].workspaces[0].chats[1])
                }
                Section("Working") {
                    ChatRowView(chat: Sample.computers[0].workspaces[0].chats[0])
                }
            }
            .navigationTitle("Home")
            .confirmationDialog("Forget studio?", isPresented: $confirming, titleVisibility: .visible) {
                Button("Forget studio", role: .destructive) {}
                Button("Cancel", role: .cancel) {}
            } message: {
                Text(forgetMessage)
            }
            .onAppear { confirming = state == 1 }
        }
    }
}

// MARK: D, Edit in Settings

struct ForgetEditList: View {
    var state: Int

    var body: some View {
        NavigationStack {
            List {
                Section {
                    ForEach(machines) { machine in MachineRow(machine: machine) }
                        .onDelete { _ in }
                } header: {
                    HStack {
                        Text("Computers")
                        Spacer()
                        Text(state == 1 ? "Done" : "Edit").textCase(nil).foregroundStyle(.tint)
                    }
                } footer: {
                    Text("Forgetting a computer removes its chats from this iPhone. It does not stop Herdr there.")
                }
                Section("Notifications") {
                    Toggle("Questions and permissions", isOn: .constant(true))
                }
            }
            .environment(\.editMode, .constant(state == 1 ? .active : .inactive))
            .navigationTitle("Settings")
        }
    }
}
