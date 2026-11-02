import SwiftUI

// Settings. A: one grouped list, like the system's. B: per computer, with
// a picker on top and app-wide settings below. C: status cards on top of
// the list, so what is happening now comes first.

struct SettingsGrouped: View {
    var body: some View {
        NavigationStack {
            List {
                Section("Computers") {
                    ForEach(Sample.computers) { computer in
                        NavigationLink {} label: {
                            HStack(spacing: 12) {
                                Image(systemName: computer.symbol).frame(width: 28)
                                VStack(alignment: .leading, spacing: 1) {
                                    Text(computer.name)
                                    Text(computer.via).font(.caption).foregroundStyle(.secondary)
                                }
                            }
                        }
                    }
                    Label("Add a computer", systemImage: "plus").foregroundStyle(.tint)
                }
                Section {
                    Toggle("Questions and permissions", isOn: .constant(true))
                    Toggle("Finished turns", isOn: .constant(true))
                    Toggle("Hold while I'm at my desk", isOn: .constant(true))
                    LabeledContent("Show on lock screen", value: "Chat name only")
                    Toggle("Live Activities", isOn: .constant(true))
                } header: {
                    Text("Notifications")
                } footer: {
                    Text("Notifications are encrypted on your computer; Apple only sees that one arrived.")
                }
                Section {
                    Toggle("Bluetooth presence", isOn: .constant(true))
                    LabeledContent("Near", value: "MacBook Pro")
                } header: {
                    Text("Presence")
                } footer: {
                    Text("Your computers know when this iPhone is close, so alerts wait until you walk away.")
                }
                Section("Security") {
                    Toggle("Require Face ID to open", isOn: .constant(false))
                    Toggle("Face ID for Always allow", isOn: .constant(true))
                }
                Section("Storage") {
                    LabeledContent("Cached chats", value: "12.4 MB")
                    Button("Clear cache", role: .destructive) {}
                }
                Section {
                    LabeledContent("Version", value: "1.0 (214)")
                    Label("Send diagnostics", systemImage: "stethoscope")
                }
            }
            .navigationTitle("Settings")
        }
    }
}

struct SettingsPerComputer: View {
    var body: some View {
        NavigationStack {
            List {
                Section {
                    Picker("Computer", selection: .constant("MacBook Pro")) {
                        ForEach(Sample.computers) { Text($0.name).tag($0.name) }
                    }
                    .pickerStyle(.segmented)
                    .listRowBackground(Color.clear)
                    .listRowInsets(EdgeInsets())
                }
                Section("Reached at") {
                    AddressRow(kind: "Wi-Fi", address: "192.168.1.20:8443", active: true)
                    AddressRow(kind: "Tailscale", address: "100.88.4.2:8443", active: false)
                    AddressRow(kind: "Bonjour", address: "MacBook-Pro.local", active: false)
                    AddressRow(kind: "Bluetooth", address: "Fallback when nothing else answers", active: false)
                }
                Section("Notifications from MacBook Pro") {
                    Toggle("Questions and permissions", isOn: .constant(true))
                    Toggle("Finished turns", isOn: .constant(false))
                    Toggle("Hold while I'm at this Mac", isOn: .constant(true))
                }
                Section("This pairing") {
                    LabeledContent("Presence", value: "Near · strong signal")
                    LabeledContent("Paired", value: "Oct 28, 2026")
                    LabeledContent("Fingerprint") {
                        Text("7F3A 91C2 04BD E1A8").font(.caption.monospaced())
                    }
                    Button("Unpair MacBook Pro", role: .destructive) {}
                }
                Section("All computers") {
                    NavigationLink("Security and Face ID") {}
                    NavigationLink("Storage") {}
                    NavigationLink("About") {}
                }
            }
            .navigationTitle("Settings")
        }
    }
}

private struct AddressRow: View {
    var kind: String
    var address: String
    var active: Bool

    var body: some View {
        HStack {
            VStack(alignment: .leading, spacing: 1) {
                Text(kind)
                Text(address).font(.caption.monospaced()).foregroundStyle(.secondary)
            }
            Spacer()
            if active {
                Text("In use").font(.caption.weight(.semibold)).foregroundStyle(.green)
            }
        }
    }
}

struct SettingsStatus: View {
    var body: some View {
        NavigationStack {
            List {
                Section {
                    LazyVGrid(columns: [GridItem(.flexible()), GridItem(.flexible())], spacing: 10) {
                        StatusCard(symbol: "dot.radiowaves.left.and.right", tint: .green, title: "At your desk", detail: "Near MacBook Pro")
                        StatusCard(symbol: "bell.badge", tint: .orange, title: "Alerts held", detail: "2 waiting · sent when you leave")
                        StatusCard(symbol: "lock.shield", tint: .blue, title: "3 computers", detail: "All pinned, last seen now")
                        StatusCard(symbol: "internaldrive", tint: .gray, title: "12.4 MB", detail: "Cached for instant launch")
                    }
                    .listRowBackground(Color.clear)
                    .listRowInsets(EdgeInsets())
                }
                Section {
                    NavigationLink { } label: { Label("Computers", systemImage: "desktopcomputer") }
                    NavigationLink { } label: { Label("Notifications", systemImage: "bell") }
                    NavigationLink { } label: { Label("Presence", systemImage: "dot.radiowaves.left.and.right") }
                    NavigationLink { } label: { Label("Security", systemImage: "faceid") }
                    NavigationLink { } label: { Label("Storage", systemImage: "internaldrive") }
                }
                Section {
                    NavigationLink { } label: { Label("Send diagnostics", systemImage: "stethoscope") }
                    NavigationLink { } label: { Label("About Herdr Remote", systemImage: "info.circle") }
                }
            }
            .navigationTitle("Settings")
        }
    }
}

private struct StatusCard: View {
    var symbol: String
    var tint: Color
    var title: String
    var detail: String

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Image(systemName: symbol).font(.title3).foregroundStyle(tint)
            Text(title).font(.headline)
            Text(detail).font(.caption).foregroundStyle(.secondary).lineLimit(2)
        }
        .frame(maxWidth: .infinity, minHeight: 104, alignment: .topLeading)
        .padding(14)
        .background(.background.secondary, in: .rect(cornerRadius: 20))
    }
}
