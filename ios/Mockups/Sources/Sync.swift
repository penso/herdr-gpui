import SwiftUI

// Where the phone says something it sent has not come back. Nothing shows
// while everything is in. State 0: Home, a mode change on its way
// (orange). State 1: a chat, the change refused and undone (red).
// A: a 3 pt bar along the screen's top edge, on every screen.
// B: the bar under the status bar, with one line saying what failed.
// C: a badge on the Computers tab and a row at the top of that tab.
// D: a small capsule hanging under the Dynamic Island.

private enum Sync {
    case sending, failed

    init(_ state: Int) { self = state == 0 ? .sending : .failed }

    var color: Color { self == .sending ? .orange : .red }
}

/// Home in its tab bar, or the chat screen, as the state asks.
private struct SyncScreen<Overlay: View>: View {
    var state: Int
    var computersBadge: String?
    @ViewBuilder var overlay: (Sync) -> Overlay

    var body: some View {
        let sync = Sync(state)
        Group {
            if sync == .sending {
                TabView {
                    Tab("Home", systemImage: "house") { AttentionList() }
                    Tab("Board", systemImage: "square.grid.2x2") { Text("Board") }
                    Tab("Computers", systemImage: "desktopcomputer") { Text("Computers") }
                        .badge(computersBadge.map { Text($0) })
                }
            } else {
                ChatDocument()
            }
        }
        .overlay(alignment: .top) { overlay(sync) }
    }
}

/// A thin bar: an orange sweep while sending, solid red once failed.
private struct SyncBar: View {
    var sync: Sync

    var body: some View {
        Group {
            if sync == .sending {
                LinearGradient(colors: [.orange.opacity(0.25), .orange, .orange.opacity(0.25)], startPoint: .leading, endPoint: .trailing)
            } else {
                Color.red
            }
        }
        .frame(height: 3)
    }
}

/// A: the bar on the screen's very top edge, above the status bar. A tap
/// would open the list of what is outstanding.
struct SyncEdge: View {
    var state: Int

    var body: some View {
        SyncScreen(state: state) { sync in
            SyncBar(sync: sync).ignoresSafeArea()
        }
    }
}

/// B: the bar just under the status bar; a failure adds one line that
/// says what was undone and slides away after a few seconds.
struct SyncStrip: View {
    var state: Int

    var body: some View {
        SyncScreen(state: state) { sync in
            VStack(spacing: 0) {
                SyncBar(sync: sync)
                if sync == .failed {
                    HStack(spacing: 8) {
                        Image(systemName: "exclamationmark.circle.fill").foregroundStyle(.red)
                        Text("Bypass Permissions did not go through · undone").font(.footnote.weight(.medium))
                        Spacer(minLength: 4)
                        Text("Retry").font(.footnote.weight(.semibold)).foregroundStyle(.tint)
                    }
                    .padding(.horizontal, 16).padding(.vertical, 8)
                    .background(.regularMaterial)
                }
            }
        }
    }
}

/// C: the Computers tab carries it, as a badge and a row on that tab.
/// Chats hide the tab bar, so state 1 shows the tab itself.
struct SyncTab: View {
    var state: Int

    var body: some View {
        let sync = Sync(state)
        TabView(selection: .constant(sync == .sending ? 0 : 2)) {
            Tab("Home", systemImage: "house", value: 0) { AttentionList() }
            Tab("Board", systemImage: "square.grid.2x2", value: 1) { Text("Board") }
            Tab("Computers", systemImage: "desktopcomputer", value: 2) {
                NavigationStack {
                    List {
                        Section {
                            HStack(spacing: 10) {
                                Image(systemName: "exclamationmark.circle.fill").foregroundStyle(.red).font(.title3)
                                VStack(alignment: .leading, spacing: 2) {
                                    Text("1 change did not go through").font(.subheadline.weight(.semibold))
                                    Text("Bypass Permissions · Equalize splits · undone").font(.caption).foregroundStyle(.secondary)
                                }
                                Spacer()
                                Text("Retry").font(.subheadline.weight(.semibold)).foregroundStyle(.tint)
                            }
                        }
                        Section("Computers") {
                            ForEach(Sample.computers) { computer in
                                Label(computer.name, systemImage: computer.symbol)
                            }
                        }
                    }
                    .navigationTitle("Computers")
                }
            }
            .badge(sync == .sending ? Text("1") : Text("!"))
        }
    }
}

/// D: a capsule just under the Dynamic Island, only while something is
/// out; on every screen, clear of the navigation bar's buttons.
struct SyncIsland: View {
    var state: Int

    var body: some View {
        SyncScreen(state: state) { sync in
            HStack(spacing: 6) {
                if sync == .sending {
                    ProgressView().controlSize(.mini).tint(.orange)
                    Text("Sending 1 change")
                } else {
                    Image(systemName: "exclamationmark.circle.fill").foregroundStyle(.red)
                    Text("1 change undone")
                }
            }
            .font(.caption.weight(.semibold))
            .padding(.horizontal, 10).padding(.vertical, 5)
            .glassEffect(.regular.tint(sync.color.opacity(0.25)), in: .capsule)
            .padding(.top, 2)
        }
    }
}
