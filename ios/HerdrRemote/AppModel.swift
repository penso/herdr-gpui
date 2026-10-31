import ActivityKit
import Foundation
import os
import HerdrRemoteCore
import Network
import UIKit

/// Where the app should go next: a chat or terminal opened from a Live
/// Activity, a notification, or a list.
private let log = Logger(subsystem: "so.pen.herdr.remote", category: "app")

enum Route: Codable, Hashable {
    case chat(PaneKey, title: String)
    case term(PaneKey, title: String)
}

/// The app's state: paired computers, their live links, and the tokens
/// Live Activities need.
@MainActor
@Observable
final class AppModel {
    private(set) var computers: [Computer] = [] {
        didSet {
            tokens.computers = computers
            fleet.sync(computers)
            presence.computers = computers
        }
    }
    var message: String?
    /// Set by a deep link; the root view shows it and clears it.
    var pendingRoute: Route?
    /// A new workspace or chat being set up, as a sheet over every tab.
    var creating: Creating?
    /// A tab to switch to, from a Control or an intent; the root view
    /// shows it and clears it.
    var pendingTab: RootTab?
    /// Something to tell the person once, such as a first prompt that
    /// did not go; the root view shows it and clears it.
    var notice: String?
    /// What each chat's composer held when the person left it, kept
    /// until they come back, for as long as the app runs.
    @ObservationIgnored var drafts: [PaneKey: ComposerDraft] = [:]

    /// Spotlight follows the app lock as soon as it changes, not at the
    /// next change to the chats.
    func appLockChanged() {
        spotlight.update(WidgetSummary.load())
    }

    /// The one model, for intents iOS runs in the app's process.
    static var current: AppModel?

    enum Shortcut { case home, newWorkspace, chat(Route) }

    /// Shows what a Control, an intent, or Spotlight asked for.
    func show(_ shortcut: Shortcut) {
        switch shortcut {
        case .home: pendingTab = .home
        case .newWorkspace: creating = .workspace(device: nil)
        case let .chat(route): pendingRoute = route
        }
    }
    let sessions: Sessions
    let tokens: ActivityTokens
    let fleet: Fleet
    /// Pairing with the hosts each computer's Herdr UI knows.
    let hostLinks: HostLinks
    let nearby = NearbyBrowser()
    let shortcuts = ShortcutStore()
    let bluetooth = NearbyBluetooth()
    /// Bluetooth presence with the computers that have it on.
    let presence = PresenceCentral()
    private let spotlight = SpotlightChats()
    /// Nearby computers the person closed the card for, this launch.
    var dismissedNearby: Set<String> = []
    private let paths = NWPathMonitor()

    init() {
        let sessions = Sessions()
        self.sessions = sessions
        tokens = ActivityTokens(sessions: sessions)
        fleet = Fleet(sessions: sessions)
        hostLinks = HostLinks(sessions: sessions)
        do {
            computers = try ComputerStore.load()
        } catch {
            log.error("Could not read the paired computers: \(error.localizedDescription, privacy: .public)")
            message = error.localizedDescription
        }
        // Property observers do not run during `init`.
        tokens.computers = computers
        fleet.sync(computers)
        presence.onAddresses = { [weak self] fingerprint, origins in self?.update(fingerprint, origins: origins) }
        // Back in range after iOS relaunched the app: tokens may be stale.
        presence.onWake = { [weak self] in
            guard let self else { return }
            Task { await self.tokens.sync() }
        }
        presence.computers = computers
        hostLinks.computers = { [weak self] in self?.computers ?? [] }
        hostLinks.onPaired = { [weak self] in self?.add($0) }
        fleet.onOnline = { [weak self] in self?.hostLinks.check($0) }
        fleet.onSummary = { [spotlight] in
            let summary = WidgetSummary.load()
            spotlight.update(summary)
            WatchLink.shared.publish(summary)
        }
        // At launch too: Spotlight may hold chats from before the lock.
        spotlight.update(WidgetSummary.load())
        WatchLink.shared.start()
        paths.pathUpdateHandler = { [weak self] _ in
            Task { @MainActor in self?.fleet.networkChanged() }
        }
        paths.start(queue: .main)
    }

    func add(_ computer: Computer) {
        // A computer is its CA: pairing the same CA (or the same device id)
        // again replaces the old record. Names are display only.
        computers.removeAll { $0.fingerprint == computer.fingerprint || $0.device == computer.device }
        computers.append(computer)
        save()
        Task { await AppDelegate.enableAlerts() }
        Task { await tokens.sync(only: computer.fingerprint) }
        hostLinks.check(computer, force: true)
    }

    /// Turns Bluetooth presence on or off for one computer.
    func setPresence(_ on: Bool, for computer: Computer) {
        guard let index = computers.firstIndex(where: { $0.device == computer.device }) else { return }
        computers[index].presence = on
        save()
    }

    /// Addresses a computer sent over Bluetooth, sealed to this phone.
    private func update(_ fingerprint: Fingerprint, origins: [String]) {
        guard let index = computers.firstIndex(where: { $0.fingerprint == fingerprint }) else { return }
        let merged = Presence.merged(known: computers[index].origins, received: origins)
        guard merged != computers[index].origins else { return }
        computers[index].origins = merged
        save()
        fleet.networkChanged()
    }

    func forget(_ computer: Computer) {
        computers.removeAll { $0.device == computer.device }
        save()
        try? NotifyKey.forget(computer.fingerprint)
        hostLinks.forget(computer)
        Task { await sessions.forget(computer) }
    }

    private func save() {
        do {
            try ComputerStore.save(computers)
        } catch {
            log.error("Could not save the paired computers: \(error.localizedDescription, privacy: .public)")
            message = error.localizedDescription
        }
    }

    /// Opens a Live Activity's or notification's link in the app.
    func open(_ url: URL) {
        guard let link = ChatLink(url: url) else { return }
        guard let fingerprint = link.fingerprint,
              let computer = computers.first(where: { $0.fingerprint == fingerprint })
        else {
            message = "\(link.computer.isEmpty ? "That computer" : link.computer) is not paired with this phone."
            return
        }
        let title = fleet.chatItems.first { $0.computer.device == computer.device && $0.chat.pane == link.pane }?.chat.title
        pendingRoute = .chat(PaneKey(device: computer.device, pane: link.pane), title: title ?? "Chat")
    }

    /// A computer nearby with its pairing dialog open that this phone has
    /// not paired, for the proximity card.
    var nearbyOffer: NearbyComputer? {
        // The network first when a computer is found both ways: it is faster.
        (nearby.computers + bluetooth.computers).first { candidate in
            candidate.pairing && !dismissedNearby.contains(candidate.id)
                && !computers.contains { candidate.matches($0.fingerprint) }
        }
    }

    /// The name to show for `computer`: its own, plus the start of its
    /// fingerprint when another paired computer has the same name.
    func displayName(_ computer: Computer) -> String {
        let twins = computers.filter { $0.name == computer.name }.count
        guard twins > 1 else { return computer.name }
        return "\(computer.name) (\(computer.fingerprint.hex.prefix(8)))"
    }

    /// The paired computer that vouched for `computer`, for one paired
    /// through another.
    func vouching(_ computer: Computer) -> Computer? {
        guard let via = computer.via else { return nil }
        return computers.first { $0.fingerprint == via }
    }

    /// The paired computer an activity names by fingerprint, if any.
    func computer(for facts: ChatActivityFacts) -> Computer? {
        guard let fingerprint = facts.fingerprint else { return nil }
        return computers.first { $0.fingerprint == fingerprint }
    }

    #if DEBUG
    /// Where the sample activity is, for Settings: iOS shows activities
    /// only outside the app, so the button says what happened.
    var sampleStatus: String?

    /// Starts a local activity with sample content and plays it through a
    /// few steps to done, to check the layouts and updates on a device
    /// without an APNs key.
    func startSample() {
        let facts = ChatActivityFacts(
            computer: computers.first?.name ?? "studio",
            computerId: computers.first?.fingerprint.hex ?? String(repeating: "0", count: 64),
            workspace: "herdr-gpui",
            chatTitle: "split tests",
            agent: "Codex",
            pane: "sample"
        )
        let state = ChatActivityState(
            phase: .working,
            step: "Writing mockups/index.html",
            startedAt: Date().addingTimeInterval(-250),
            planDone: 2,
            planTotal: 4,
            working: 2,
            waiting: 1
        )
        let id: String
        do {
            id = try Activity.request(
                attributes: ChatActivityAttributes(facts: facts),
                content: .init(state: state, staleDate: nil)
            ).id
        } catch {
            sampleStatus = "Could not start: \(error.localizedDescription)"
            return
        }
        sampleStatus = "Started. Go to the Home Screen or lock the phone: it shows in the Dynamic Island and on the Lock Screen, and updates every few seconds."
        // The app is suspended soon after it leaves the screen, which is
        // when the activity shows; the updates need the background time.
        let background = UIApplication.shared.beginBackgroundTask(withName: "Sample activity")
        Task { [weak self] in
            await Self.playSample(id, from: state)
            self?.sampleStatus = "Finished. It leaves the Lock Screen in a minute."
            UIApplication.shared.endBackgroundTask(background)
        }
    }

    /// Steps the sample activity `id` through a few updates, then ends it.
    private nonisolated static func playSample(_ id: String, from start: ChatActivityState) async {
        var state = start
        for (index, step) in ["Running cargo test", "Fixing split_tests.rs", "Running just ci"].enumerated() {
            try? await Task.sleep(for: .seconds(5))
            state.step = step
            state.planDone = min(3 + index, 4)
            await Activity<ChatActivityAttributes>.activities.first { $0.id == id }?.update(.init(state: state, staleDate: nil))
        }
        try? await Task.sleep(for: .seconds(5))
        state.phase = .finished
        state.summary = "Split the tests into four files; just ci passes."
        state.working = 1
        await Activity<ChatActivityAttributes>.activities.first { $0.id == id }?
            .end(.init(state: state, staleDate: nil), dismissalPolicy: .after(.now + 60))
    }
    #endif
}
