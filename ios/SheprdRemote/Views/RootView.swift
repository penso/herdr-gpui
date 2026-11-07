import SheprdRemoteCore
import SwiftUI

enum RootTab: Hashable { case home, workspaces, settings }

/// Home B: attention first, with Workspaces and its search, and Settings,
/// as tabs, or
/// iPad B's split view when the window is wide: an iPad, or a foldable
/// iPhone opened flat.
struct RootView: View {
    @Environment(AppModel.self) private var model
    @State private var tab = RootTab.home
    @State private var homePath: [Route] = []
    @State private var workspacesPath = NavigationPath()
    @State private var selection: Route?
    @State private var windowWidth: CGFloat = 0
    @State private var window = WindowState()
    @Environment(\.horizontalSizeClass) private var sizeClass
    @Environment(\.scenePhase) private var scenePhase

    /// The width from which a window gets the split view. An unfolded
    /// iPhone can report a compact size class while it has an iPad mini's
    /// width, so the window's own width decides too.
    static let wideWidth: CGFloat = 600

    private var wide: Bool { sizeClass == .regular || windowWidth >= Self.wideWidth }

    var body: some View {
        Group {
            if wide {
                // The split view collapses to one column in a compact size
                // class, and the chat lays out its header by it.
                IPadRoot(pairing: $window.pairing, selection: $selection)
                    .environment(\.horizontalSizeClass, .regular)
            } else {
                tabs
            }
        }
        .onGeometryChange(for: CGFloat.self) { $0.size.width } action: { windowWidth = $0 }
        .environment(window)
        // Along the top edge on every screen; sheets carry their own.
        .overlay(alignment: .top) { SyncBar().ignoresSafeArea(edges: .top) }
        // Folding or unfolding keeps the open chat or terminal open.
        .onChange(of: wide) { _, nowWide in
            if nowWide {
                if let route = homePath.last { selection = route }
                homePath = []
            } else if let route = selection {
                tab = .home
                homePath = [route]
            }
        }
        // The split view presents it itself, over whichever sheet is open.
        .sheet(isPresented: Binding(get: { window.pairing && !wide }, set: { window.pairing = $0 })) { PairView() }
        .alert("Not sent", isPresented: Binding(get: { model.notice != nil }, set: { if !$0 { model.notice = nil } })) {
        } message: {
            Text(model.notice ?? "")
        }
        .sheet(item: Bindable(model).creating) { creating in
            switch creating {
            case let .workspace(device, folder): NewWorkspaceSheet(device: device, folder: folder)
            case let .chat(device, workspace, label, agent): NewChatSheet(device: device, workspace: workspace, label: label, agent: agent)
            }
        }
        // A panel's sheet draws it instead, as here it would be behind it.
        .nearbyCard(window, when: !window.panelOpen)
        // A first computer gets the card at once; later ones, the banner.
        .onChange(of: model.nearbyOffer?.id, initial: true) {
            if model.computers.isEmpty, window.nearby == nil { window.nearby = model.nearbyOffer }
        }
        .onChange(of: model.offeredNearby) { _, offered in
            model.dismissedNearby.formIntersection(offered)
            // Its dialog closed there. Once paired, the card finishes first.
            if let shown = window.nearby, !offered.contains(shown.fingerprintPrefix),
               !model.computers.contains(where: { shown.matches($0.fingerprint) }) {
                window.nearby = nil
            }
        }
        // Bluetooth looks for computers while pairing is likely, so its
        // permission prompt comes when the person is pairing, and once
        // allowed, whenever the app is in front, to offer new ones. Shared
        // by every window: each says whether it wants it.
        .onChange(of: lookingOverBluetooth, initial: true) { _, looking in
            model.bluetooth.look(looking, for: window)
        }
        .onDisappear { model.bluetooth.look(false, for: window) }
        #if DEBUG
        .task { await followLaunchArguments() }
        #endif
        .onChange(of: model.pendingTab) { _, wanted in
            guard let wanted else { return }
            tab = wanted
            model.pendingTab = nil
        }
        .onChange(of: model.pendingRoute) { _, route in
            guard let route else { return }
            if wide {
                selection = route
            } else {
                tab = .home
                homePath.append(route)
            }
            model.pendingRoute = nil
        }
        .onChange(of: model.pendingWorkspace) { _, key in
            guard let key else { return }
            showWorkspace(key)
            model.pendingWorkspace = nil
        }
    }

    /// Goes back to the workspace when it is in the stack, else makes it
    /// the whole stack, so Back from it returns to Home whatever was open.
    private func showWorkspace(_ key: WorkspaceKey) {
        let route = Route.workspace(key)
        if let index = homePath.firstIndex(of: route) {
            homePath.removeSubrange((index + 1)...)
        } else {
            homePath = [route]
        }
    }

    private var lookingOverBluetooth: Bool {
        NearbyScan(
            pairingLikely: window.pairing || model.computers.isEmpty,
            exchanging: window.exchanging,
            foreground: scenePhase == .active,
            allowed: NearbyBluetooth.allowed
        ).looking
    }

    private var tabs: some View {
        TabView(selection: $tab) {
            Tab("Home", systemImage: "house", value: RootTab.home) {
                NavigationStack(path: $homePath) {
                    HomeTab(pairing: $window.pairing).withRoutes()
                }
                .environment(\.offersWorkspace, true)
                .slidesTabBar(hidden: !homePath.isEmpty)
                .connectionToast(pairing: $window.pairing)
            }
            .badge(model.fleet.waiting)
            Tab("Workspaces", systemImage: "square.stack.3d.up", value: RootTab.workspaces) {
                NavigationStack(path: $workspacesPath) { WorkspacesTab().withRoutes() }
                    .slidesTabBar(hidden: !workspacesPath.isEmpty)
                    .connectionToast(pairing: $window.pairing)
            }
            Tab("Settings", systemImage: "gearshape", value: RootTab.settings) {
                NavigationStack { SettingsView() }
                    .connectionToast(pairing: $window.pairing)
            }
        }
    }
}

#if DEBUG
extension RootView {
    /// For the simulator loop (`ios/README.md`), which cannot tap:
    /// `-pairLink URL` pairs without the camera (the desktop still
    /// approves), `-openLink URL` opens a Live Activity's link as a tap on
    /// the activity would, `-openTerm PANE` opens the terminal pane of
    /// whichever computer lists it, `-tab workspaces|settings` picks
    /// that tab, `-openWorkspace ID` opens that workspace in the Workspaces
    /// tab, `-newWorkspace YES` opens the new workspace form,
    /// `-openComputer YES` opens the first computer's page from Workspaces,
    /// and `-editComputers YES` puts Settings' computers in Edit. In a
    /// chat, `-session YES` opens its session sheet and `-draft TEXT`
    /// types into the composer.
    func followLaunchArguments() async {
        let defaults = UserDefaults.standard
        if let link = defaults.string(forKey: "pairLink"),
           !model.computers.contains(where: { (try? PairingLink(link))?.fingerprint == $0.fingerprint }) {
            let pairing = PairingModel()
            pairing.pair(link, deviceName: "Simulator") { model.add($0) }
        }
        if let text = defaults.string(forKey: "openLink"), let url = URL(string: text) {
            // Lets the first home arrive, so the chat has its title.
            try? await Task.sleep(for: .seconds(2))
            model.open(url)
        }
        switch defaults.string(forKey: "tab") {
        case "workspaces": tab = .workspaces
        case "settings": tab = .settings
        default: break
        }
        if defaults.bool(forKey: "newWorkspace") {
            try? await Task.sleep(for: .seconds(2))
            model.creating = .workspace(device: nil)
        }
        if let workspace = defaults.string(forKey: "openWorkspace") {
            try? await Task.sleep(for: .seconds(2))
            if let machine = model.fleet.machines.first(where: { $0.home?.workspaces.contains { $0.id == workspace } == true }) {
                tab = .workspaces
                workspacesPath.append(WorkspaceKey(device: machine.id, workspace: workspace))
            }
        }
        if let pane = defaults.string(forKey: "openTerm") {
            try? await Task.sleep(for: .seconds(2))
            if let item = model.fleet.terminalItems.first(where: { $0.terminal.panes.contains { $0.pane == pane } }) {
                model.pendingRoute = .term(PaneKey(device: item.computer.device, pane: pane), title: item.terminal.label)
            }
        }
    }
}
#endif

extension View {
    func withRoutes() -> some View {
        navigationDestination(for: Route.self) { $0.screen }
    }

    /// Slides a tab's bar in and out with its stack. The screens pushed on
    /// the stack hide the bar themselves; on their own, it snaps back when
    /// they go.
    func slidesTabBar(hidden: Bool) -> some View {
        toolbar(hidden ? .hidden : .visible, for: .tabBar)
            .animation(.default, value: hidden)
    }
}

extension Route {
    @ViewBuilder var screen: some View {
        switch self {
        case let .chat(key, title): ChatScreen(key: key, title: title)
        case let .term(key, title): TerminalScreen(key: key, title: title)
        case let .workspace(key): WorkspaceScreen(key: key)
        case let .changes(key): ChangesScreen(key: key)
        }
    }
}
