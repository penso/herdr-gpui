import HerdrRemoteCore
import SwiftUI

/// iPad B: the attention list beside the open chat, with the chat's
/// workspace terminal in an inspector. Workspaces and Settings open
/// as sheets; a chat or terminal can also open in a window of its own.
/// A foldable iPhone opened flat gets it too (`RootView.wideWidth`).
struct IPadRoot: View {
    @Environment(AppModel.self) private var model
    @Binding var pairing: Bool
    @Binding var selection: Route?
    @State private var query = ""
    @State private var showing: Panel?

    enum Panel: String, Identifiable {
        case workspaces, settings
        var id: String { rawValue }
    }

    var body: some View {
        NavigationSplitView {
            AttentionList(selection: $selection, query: query)
                .connectionToast(pairing: $pairing)
                .searchable(text: $query, placement: .sidebar, prompt: "Chats, messages, terminals")
                .navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .topBarTrailing) {
                        Menu {
                            Button("New Workspace", systemImage: "folder.badge.plus") { model.creating = .workspace(device: nil) }
                                .keyboardShortcut("n", modifiers: .command)
                                .disabled(model.fleet.machines.isEmpty)
                            Button("Pair a Computer", systemImage: "laptopcomputer.and.iphone") { pairing = true }
                        } label: { Image(systemName: "plus") }
                        .accessibilityLabel("New")
                    }
                    ToolbarItemGroup(placement: .bottomBar) {
                        Spacer()
                        Button("Workspaces", systemImage: "square.stack.3d.up") { showing = .workspaces }
                        Button("Settings", systemImage: "gearshape") { showing = .settings }
                            .keyboardShortcut(",", modifiers: .command)
                    }
                }
        } detail: {
            switch selection {
            case let .chat(key, title)?:
                ChatWithTerminal(key: key, title: title).id(key)
            case let .term(key, title)?:
                NavigationStack { TerminalScreen(key: key, title: title) }.id(key)
            case nil:
                ContentUnavailableView("Pick a chat", systemImage: "bubble.left.and.text.bubble.right",
                                       description: Text("What needs you is at the top of the list."))
            }
        }
        .background { ChatKeys(selection: $selection) }
        .sheet(item: $showing) { panel in
            NavigationStack {
                switch panel {
                case .workspaces: WorkspacesTab(pairing: $pairing).withRoutes()
                case .settings: SettingsView()
                }
            }
            .connectionToast(pairing: $pairing)
            .presentationSizing(.page)
        }
    }
}

/// The sidebar: what needs you, what works, the rest, then terminals.
private struct AttentionList: View {
    @Environment(AppModel.self) private var model
    @Environment(\.openWindow) private var openWindow
    @Binding var selection: Route?
    let query: String
    @State private var messages: [MessageHit] = []

    var body: some View {
        let chats = model.fleet.chatItems.filter(matches)
        let terminals = model.fleet.terminalItems.filter(matches)
        // Tabs with an agent list with the chats; the rest under Terminals.
        let agents = terminals.filter { $0.agent != nil }
        let shells = terminals.filter { $0.agent == nil }
        List(selection: $selection) {
            section("Needs you", chats.filter(\.needsYou), agents.filter(\.needsYou))
            section("Working", chats.filter(\.working), agents.filter(\.working))
            section("Recent", chats.filter { !$0.needsYou && !$0.working }, agents.filter { !$0.needsYou && !$0.working })
            if !query.isEmpty, !messages.isEmpty {
                Section("Messages") {
                    ForEach(messages) { hit in
                        let route = Route.chat(hit.item.key, title: hit.item.chat.title)
                        MessageHitRow(hit: hit, query: query).tag(route).contextMenu { windowButton(route) }
                    }
                }
            }
            if !shells.isEmpty {
                Section("Terminals") {
                    ForEach(shells) { item in
                        if let pane = item.terminal.firstPane {
                            let route = Route.term(PaneKey(device: item.computer.device, pane: pane.pane), title: item.terminal.label)
                            Label("\(item.terminal.label) · \(item.workspace.label)", systemImage: "apple.terminal")
                                .lineLimit(1)
                                .tag(route)
                                .contextMenu { windowButton(route) }
                        }
                    }
                }
            }
        }
        .listStyle(.sidebar)
        .refreshable { model.fleet.networkChanged() }
        .task(id: query) {
            let found = await model.fleet.searchMessages(query)
            guard !Task.isCancelled else { return }
            messages = found
        }
    }

    @ViewBuilder
    private func section(_ title: String, _ items: [ChatItem], _ agents: [TerminalItem]) -> some View {
        if !items.isEmpty || !agents.isEmpty {
            Section(title) {
                ForEach(items) { item in
                    let route = Route.chat(item.key, title: item.chat.title)
                    ChatRowLabel(item: item).tag(route).contextMenu { windowButton(route) }
                }
                ForEach(agents) { item in
                    if let route = item.route {
                        AgentTerminalRowLabel(item: item).tag(route).contextMenu { windowButton(route) }
                    }
                }
            }
        }
    }

    private func windowButton(_ route: Route) -> some View {
        Button("Open in New Window", systemImage: "macwindow.badge.plus") {
            openWindow(value: route)
        }
    }

    private func matches(_ item: ChatItem) -> Bool {
        query.isEmpty || item.chat.title.localizedCaseInsensitiveContains(query) || item.workspace.label.localizedCaseInsensitiveContains(query)
    }

    private func matches(_ item: TerminalItem) -> Bool {
        query.isEmpty || item.title.localizedCaseInsensitiveContains(query) || item.terminal.label.localizedCaseInsensitiveContains(query)
            || item.workspace.label.localizedCaseInsensitiveContains(query)
    }
}

/// ⌘⌥↑ and ⌘⌥↓ move through the chats in the sidebar's order.
private struct ChatKeys: View {
    @Environment(AppModel.self) private var model
    @Binding var selection: Route?

    var body: some View {
        Group {
            Button("Previous Chat") { move(-1) }.keyboardShortcut(.upArrow, modifiers: [.command, .option])
            Button("Next Chat") { move(1) }.keyboardShortcut(.downArrow, modifiers: [.command, .option])
        }
        .opacity(0)
        .accessibilityHidden(true)
    }

    private func move(_ step: Int) {
        let items = model.fleet.chatItems
        let ordered = items.filter(\.needsYou) + items.filter(\.working) + items.filter { !$0.needsYou && !$0.working }
        let routes = ordered.map { Route.chat($0.key, title: $0.chat.title) }
        guard !routes.isEmpty else { return }
        let index = selection.flatMap { routes.firstIndex(of: $0) } ?? (step > 0 ? -1 : routes.count)
        selection = routes[min(max(index + step, 0), routes.count - 1)]
    }
}

/// iPad B: the chat, with a terminal of its workspace in an inspector
/// beside it, read only (open it full to type), shown with ⌘⌥I. The
/// inspector sits outside the chat's navigation stack, so the chat keeps
/// its own header.
private struct ChatWithTerminal: View {
    @Environment(AppModel.self) private var model
    let key: PaneKey
    let title: String
    /// Set once the person shows or hides the terminal; until then it
    /// shows when the chat keeps a comfortable width beside it.
    @State private var chosen: Bool?
    @State private var width: CGFloat = 0
    @State private var tab: String?
    @State private var path = NavigationPath()

    var body: some View {
        let terminals = model.fleet.terminalItems.filter { item in
            item.computer.device == key.device
                && model.fleet.chatItems.contains { $0.key == key && $0.workspace.id == item.workspace.id }
        }
        // The newest tab first: usually the one started for this work,
        // rather than the workspace's default shell.
        let current = terminals.first { $0.terminal.tab == tab } ?? terminals.last
        let shown = chosen ?? (width >= 900)
        NavigationStack(path: $path) {
            ChatScreen(key: key, title: title)
                .toolbar {
                    if !terminals.isEmpty {
                        ToolbarItem(placement: .topBarTrailing) {
                            Button { chosen = !shown } label: { Image(systemName: "sidebar.trailing") }
                                .keyboardShortcut("i", modifiers: [.command, .option])
                                .accessibilityLabel(shown ? "Hide the terminal" : "Show the terminal")
                        }
                    }
                }
                .navigationDestination(for: Route.self) { route in
                    if case let .term(key, title) = route { TerminalScreen(key: key, title: title) }
                }
        }
        .inspector(isPresented: Binding(get: { shown && !terminals.isEmpty }, set: { chosen = $0 })) {
            VStack(spacing: 0) {
                if let current, let pane = current.terminal.firstPane {
                    HStack {
                        Menu {
                            ForEach(terminals) { item in
                                Button(item.terminal.label) { tab = item.terminal.tab }
                            }
                        } label: {
                            Label(current.terminal.label, systemImage: "apple.terminal").font(.subheadline.weight(.semibold))
                        }
                        Spacer()
                        Button {
                            path.append(Route.term(PaneKey(device: key.device, pane: pane.pane), title: current.terminal.label))
                        } label: {
                            Image(systemName: "arrow.up.left.and.arrow.down.right")
                        }
                        .accessibilityLabel("Open the terminal")
                    }
                    .padding(12)
                    MiniTerminal(key: PaneKey(device: key.device, pane: pane.pane)).id(pane.pane)
                }
            }
            .background(TerminalScreen.background)
            .environment(\.colorScheme, .dark)
            .inspectorColumnWidth(min: 300, ideal: 420)
        }
        // Measured with the inspector included, so showing it does not
        // shrink the width that decided to show it.
        .onGeometryChange(for: CGFloat.self) { $0.size.width } action: { width = $0 }
    }
}

/// A terminal's screen, fit to the inspector's width, read only.
private struct MiniTerminal: View {
    @Environment(AppModel.self) private var model
    let key: PaneKey
    @State private var store: TermStore?

    var body: some View {
        GeometryReader { geometry in
            if let screen = store?.screen {
                let size = TerminalScreen.fit(columns: screen.columns, width: geometry.size.width - 16)
                ScrollView {
                    VStack(alignment: .leading, spacing: 0) {
                        ForEach(Array(screen.rows.enumerated()), id: \.offset) { _, row in
                            Text(row.reduce(into: AttributedString()) { line, run in line += TerminalScreen.styled(run) })
                                .font(.system(size: size, design: .monospaced))
                                .lineLimit(1)
                                .fixedSize(horizontal: true, vertical: false)
                        }
                    }
                    .padding(8)
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
                .defaultScrollAnchor(.bottom)
            } else {
                ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .onAppear { store = model.fleet.openTerm(key) }
        .onDisappear { if let store { model.fleet.closeTerm(store) } }
    }
}
