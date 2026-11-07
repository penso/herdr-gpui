import SwiftUI

// Starting a tab from a workspace, replacing the row that scrolls sideways.
// A: every choice on show, in a grid. B: one + in the toolbar, with a menu.
// C: Terminal and the agent last used as large buttons, the rest folded.
// D: New tab as the last row of the tabs, opening a sheet with a first prompt.
// State 1 shows B, C, and D opened.

/// What a tab can start. The sample `Agent` has four; the app offers seven.
private struct Launch: Identifiable {
    var name: String
    var symbol: String
    var tint: Color
    var id: String { name }

    static let terminal = Launch(name: "Terminal", symbol: "apple.terminal", tint: Color(white: 0.45))
    static let claude = Launch(name: "Claude", symbol: "sparkle", tint: Color(red: 0.85, green: 0.47, blue: 0.34))
    static let others = [
        Launch(name: "Codex", symbol: "hexagon.fill", tint: Color(white: 0.2)),
        Launch(name: "Copilot", symbol: "person.crop.circle.fill", tint: .purple),
        Launch(name: "Gemini", symbol: "diamond.fill", tint: Color(red: 0.36, green: 0.52, blue: 0.96)),
        Launch(name: "OpenCode", symbol: "square.fill", tint: Color(white: 0.6)),
        Launch(name: "Pi", symbol: "p.circle.fill", tint: Color(white: 0.35)),
    ]
    static let all = [terminal, claude] + others
}

private struct LaunchIcon: View {
    var launch: Launch
    var size: CGFloat = 32

    var body: some View {
        Image(systemName: launch.symbol)
            .font(.system(size: size * 0.5, weight: .semibold))
            .foregroundStyle(.white)
            .frame(width: size, height: size)
            .background(launch.tint.gradient, in: .rect(cornerRadius: size * 0.28))
    }
}

private struct LaunchGrid: View {
    var body: some View {
        LazyVGrid(columns: Array(repeating: GridItem(.flexible(), spacing: 8), count: 4), spacing: 14) {
            ForEach(Launch.all) { launch in
                VStack(spacing: 5) {
                    LaunchIcon(launch: launch, size: 40)
                    Text(launch.name).font(.caption.weight(.medium)).lineLimit(1).minimumScaleFactor(0.8)
                }
            }
        }
        .padding(.vertical, 6)
    }
}

/// The workspace screen around each variant.
private struct WorkspaceList<Extra: View>: View {
    @ViewBuilder var extra: Extra
    var tabsFooter: AnyView? = nil

    var body: some View {
        List {
            Section {
                HStack(spacing: 12) {
                    Image(systemName: "arrow.triangle.branch").font(.system(size: 14, weight: .semibold))
                        .foregroundStyle(.tint)
                        .frame(width: 28, height: 28).background(.tint.opacity(0.15), in: .rect(cornerRadius: 8))
                    VStack(alignment: .leading, spacing: 2) {
                        Text("mobile-better").font(.body.monospaced())
                        Text("studio · herdr-gpui").font(.subheadline).foregroundStyle(.secondary)
                    }
                }
            }
            Section("2 tabs") {
                ForEach(SpaceSample.tabs.prefix(2)) { tab in
                    HStack(spacing: 12) {
                        AgentIcon(agent: tab.agent, size: 32)
                        VStack(alignment: .leading, spacing: 2) {
                            Text(tab.title).lineLimit(1)
                            Text(tab.detail).font(.subheadline).foregroundStyle(.secondary).lineLimit(1)
                        }
                    }
                }
                if let tabsFooter { tabsFooter }
            }
            extra
        }
        .navigationTitle("mobile-better")
        .navigationBarTitleDisplayMode(.inline)
    }
}

// MARK: A

struct NewTabGrid: View {
    var body: some View {
        NavigationStack {
            WorkspaceList {
                Section("New tab") { LaunchGrid() }
            }
        }
    }
}

// MARK: B

struct NewTabMenu: View {
    var state: Int

    var body: some View {
        NavigationStack {
            WorkspaceList { EmptyView() }
                .toolbar {
                    ToolbarItem(placement: .topBarTrailing) { Button {} label: { Image(systemName: "plus") } }
                }
                .overlay(alignment: .topTrailing) { if state == 1 { menu } }
        }
    }

    /// Drawn, as a real menu cannot open without a tap.
    private var menu: some View {
        VStack(spacing: 0) {
            ForEach(Launch.all) { launch in
                HStack(spacing: 12) {
                    Text(launch.name)
                    Spacer()
                    LaunchIcon(launch: launch, size: 26)
                }
                .padding(.horizontal, 16).padding(.vertical, 10)
                if launch.id == Launch.terminal.id { Divider() }
            }
        }
        .frame(width: 240)
        .padding(.vertical, 6)
        .glassEffect(.regular, in: .rect(cornerRadius: 24))
        .padding(.trailing, 14).padding(.top, 4)
    }
}

// MARK: C

struct NewTabQuick: View {
    var state: Int

    var body: some View {
        NavigationStack {
            WorkspaceList {
                Section("New tab") {
                    HStack(spacing: 10) {
                        big(.terminal)
                        big(.claude)
                    }
                    .listRowInsets(EdgeInsets(top: 12, leading: 12, bottom: 12, trailing: 12))
                    HStack(spacing: 12) {
                        Text("Other agents")
                        Spacer()
                        if state == 0 {
                            HStack(spacing: -6) { ForEach(Launch.others) { LaunchIcon(launch: $0, size: 22) } }
                        }
                        Image(systemName: state == 1 ? "chevron.up" : "chevron.down")
                            .font(.footnote.weight(.semibold)).foregroundStyle(.tertiary)
                    }
                    if state == 1 {
                        ForEach(Launch.others) { launch in
                            HStack(spacing: 12) { LaunchIcon(launch: launch, size: 28); Text(launch.name) }
                        }
                    }
                }
            }
        }
    }

    private func big(_ launch: Launch) -> some View {
        VStack(spacing: 8) {
            LaunchIcon(launch: launch, size: 40)
            Text(launch.name).font(.subheadline.weight(.semibold))
        }
        .frame(maxWidth: .infinity).padding(.vertical, 14)
        .background(.fill.quinary, in: .rect(cornerRadius: 16))
    }
}

// MARK: D

struct NewTabRowSheet: View {
    var state: Int
    @State private var prompt = ""

    var body: some View {
        NavigationStack {
            WorkspaceList(extra: { EmptyView() }, tabsFooter: AnyView(row))
        }
        .sheet(isPresented: .constant(state == 1)) {
            NavigationStack {
                Form {
                    Section { LaunchGrid() }
                    Section("First prompt") {
                        TextField("What should it do first? (optional)", text: $prompt, axis: .vertical).lineLimit(2...4)
                    }
                }
                .navigationTitle("New Tab")
                .navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) { Button("Cancel") {} }
                }
            }
            .presentationDetents([.medium])
        }
    }

    private var row: some View {
        HStack(spacing: 12) {
            Image(systemName: "plus").font(.system(size: 15, weight: .semibold)).foregroundStyle(.tint)
                .frame(width: 32, height: 32).background(.tint.opacity(0.15), in: .rect(cornerRadius: 9))
            Text("New tab").foregroundStyle(.tint)
        }
    }
}
