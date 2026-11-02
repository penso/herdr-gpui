import HerdrRemoteCore
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
    @State private var pairing = false
    @State private var selection: Route?
    @State private var windowWidth: CGFloat = 0
    @Environment(\.horizontalSizeClass) private var sizeClass

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
                IPadRoot(pairing: $pairing, selection: $selection)
                    .environment(\.horizontalSizeClass, .regular)
            } else {
                tabs
            }
        }
        .onGeometryChange(for: CGFloat.self) { $0.size.width } action: { windowWidth = $0 }
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
        .sheet(isPresented: $pairing) { PairView() }
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
        .overlay(alignment: .bottom) {
            if let offer = model.nearbyOffer {
                ZStack(alignment: .bottom) {
                    Color.black.opacity(0.25).ignoresSafeArea()
                    ProximityCard(computer: offer) { model.dismissedNearby.insert(offer.id) }
                        .id(offer.id)
                }
            }
        }
        .animation(.spring(duration: 0.4), value: model.nearbyOffer?.id)
        // Bluetooth looks for computers only while pairing is likely, so
        // its permission prompt comes when the person is pairing.
        .onChange(of: pairing || model.computers.isEmpty, initial: true) { _, looking in
            if looking { model.bluetooth.start() } else { model.bluetooth.stop() }
        }
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
    }

    private var tabs: some View {
        TabView(selection: $tab) {
            Tab("Home", systemImage: "house", value: RootTab.home) {
                NavigationStack(path: $homePath) {
                    HomeTab(pairing: $pairing).withRoutes()
                }
                .connectionToast(pairing: $pairing)
            }
            .badge(model.fleet.waiting)
            Tab("Workspaces", systemImage: "square.stack.3d.up", value: RootTab.workspaces) {
                NavigationStack(path: $workspacesPath) { WorkspacesTab(pairing: $pairing).withRoutes() }
                    .connectionToast(pairing: $pairing)
            }
            Tab("Settings", systemImage: "gearshape", value: RootTab.settings) {
                NavigationStack { SettingsView() }
                    .connectionToast(pairing: $pairing)
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
    /// tab, and `-newWorkspace YES` opens the new workspace form. In a
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
}

extension Route {
    @ViewBuilder var screen: some View {
        switch self {
        case let .chat(key, title): ChatScreen(key: key, title: title)
        case let .term(key, title): TerminalScreen(key: key, title: title)
        }
    }
}
