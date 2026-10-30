import Foundation
import HerdrRemoteCore
import Observation
import os
import WidgetKit

private let log = Logger(subsystem: "so.pen.herdr.remote", category: "fleet")

/// A chat row with where it lives, for lists across computers.
struct ChatItem: Identifiable, Hashable {
    let computer: Computer
    let workspace: Home.Workspace
    let chat: Home.ChatRow
    /// This phone answered its question; the answer is on its way.
    var answered = false
    /// When the agent last answered: the computer's time, else when this
    /// phone saw it.
    var lastAnswer: Date?

    var id: String { "\(computer.device)/\(chat.pane)" }
    var key: PaneKey { PaneKey(device: computer.device, pane: chat.pane) }

    /// The question waiting, with its options.
    var question: Permission? { answered ? nil : chat.summary?.question }
    var needsYou: Bool { question != nil || (!answered && chat.status == .blocked) }
    var working: Bool { !needsYou && (chat.status == .working || chat.summary?.status == .working) }
}

enum FleetError: Error, LocalizedError {
    case offline(String)

    var errorDescription: String? {
        switch self {
        case let .offline(name): "\(name) is not connected."
        }
    }
}

/// A terminal tab with where it lives.
struct TerminalItem: Identifiable, Hashable {
    let computer: Computer
    let workspace: Home.Workspace
    let terminal: Home.TerminalRow

    var id: String { "\(computer.device)/\(terminal.tab)" }

    /// The agent Herdr detects in it, such as `claude` run in a shell.
    var agent: Home.TerminalRow.DetectedAgent? { terminal.agent }
    var title: String { agent?.title ?? terminal.label }
    var needsYou: Bool { agent?.status == .blocked }
    var working: Bool { agent?.status == .working }

    /// Where a list opens it: the agent's pane, else the focused one.
    var route: Route? {
        terminal.agentPane.map { .term(PaneKey(device: computer.device, pane: $0.pane), title: terminal.label) }
    }
}

/// Every paired computer's link and what it last showed. Launch draws the
/// cached homes at once, marked stale until each link brings a fresh one.
@MainActor
@Observable
final class Fleet {
    struct Machine: Identifiable {
        var computer: Computer
        var status: LinkStatus = .connecting
        var home: Home?
        /// `home` came from the cache and the link has not refreshed it.
        var stale = true
        /// When the computer last sent `home`.
        var updated: Date?
        /// When this phone saw its chats answer, for a computer that does
        /// not date them itself.
        var answers = AnswerTimes()
        /// `home` came over the link now up, so the next one shows what
        /// changed in between as it happened.
        var watching = false
        var id: String { computer.device }
    }

    private(set) var machines: [Machine] = []
    private var links: [String: ComputerLink] = [:]
    private var readers: [String: Task<Void, Never>] = [:]
    private var chats: [PaneKey: ChatStore] = [:]
    private var terms: [PaneKey: TermStore] = [:]
    /// How many views show each chat or terminal: iPad windows can show
    /// the same one, and only the last to close stops watching it.
    @ObservationIgnored private var chatViewers: [PaneKey: Int] = [:]
    @ObservationIgnored private var termViewers: [PaneKey: Int] = [:]
    /// One client per computer, kept while its address and CA stay.
    @ObservationIgnored private var clients: [String: RemoteClient] = [:]
    /// Commands sent to chats that have not come back yet.
    let pending = PendingCommands()
    private var active = false
    let sessions: Sessions
    let cache: Cache

    init(sessions: Sessions, cache: Cache = .shared) {
        self.sessions = sessions
        self.cache = cache
    }

    /// Follows `computers`: new ones get a link, forgotten ones lose theirs.
    func sync(_ computers: [Computer]) {
        let wanted = Set(computers.map(\.device))
        for machine in machines where !wanted.contains(machine.id) {
            stopLink(machine.id)
            cache.clear(machine.id)
        }
        machines = computers.map { computer in
            if var known = machines.first(where: { $0.id == computer.device }) {
                // Addresses learned since (over Bluetooth, say) go to the
                // running link too, which otherwise keeps the old ones.
                if known.computer.origins != computer.origins, let link = links[computer.device] {
                    let origins = computer.origins
                    Task {
                        await link.updateOrigins(origins)
                        await link.networkChanged()
                    }
                }
                known.computer = computer
                return known
            }
            return Machine(
                computer: computer,
                home: cache.home(computer.device),
                updated: cache.homeDate(computer.device),
                answers: cache.answers(computer.device)
            )
        }
        if active { machines.forEach { startLink($0.computer) } }
        // From the cached homes at launch, and without forgotten ones.
        publishSummary()
    }

    /// The app came to the foreground: links connect and catch up.
    func activate() {
        active = true
        machines.forEach { startLink($0.computer) }
        for link in links.values { Task { await link.networkChanged() } }
    }

    /// The app left the foreground: sockets close; pushes cover the rest.
    func deactivate() {
        active = false
        for id in Array(links.keys) { stopLink(id) }
        for index in machines.indices {
            machines[index].stale = true
            machines[index].watching = false
        }
    }

    func networkChanged() {
        for link in links.values { Task { await link.networkChanged() } }
    }

    // MARK: Lists

    var chatItems: [ChatItem] {
        machines.flatMap { machine in
            (machine.home?.workspaces ?? []).flatMap { workspace in
                workspace.chats.map { chat in
                    let key = PaneKey(device: machine.computer.device, pane: chat.pane)
                    let answered = chat.summary?.question.map { pending.answered(key, permission: $0.id) } ?? false
                    let ended = chat.summary?.lastTurnEndedMs.map { Date(timeIntervalSince1970: Double($0) / 1000) }
                    return ChatItem(
                        computer: machine.computer,
                        workspace: workspace,
                        chat: chat,
                        answered: answered,
                        lastAnswer: ended ?? machine.answers[chat.pane]
                    )
                }
            }
        }
    }

    var terminalItems: [TerminalItem] {
        machines.flatMap { machine in
            (machine.home?.workspaces ?? []).flatMap { workspace in
                workspace.terminals.map { TerminalItem(computer: machine.computer, workspace: workspace, terminal: $0) }
            }
        }
    }

    /// Terminal tabs with an agent in them, listed beside the chats.
    var agentTerminals: [TerminalItem] { terminalItems.filter { $0.agent != nil } }

    var waiting: Int { machines.reduce(0) { $0 + ($1.home?.waiting ?? 0) } }

    /// Records each computer's waiting count where pushes find it, so a
    /// push from one computer adds to the others' counts rather than
    /// replacing them; the badge to show.
    func recordBadge() -> Int {
        let defaults = UserDefaults(suiteName: "group.so.pen.herdr.remote")
        var counts = BadgeCounts.load(from: defaults)
        for machine in machines {
            if let home = machine.home { counts.set(home.waiting, for: machine.computer.fingerprint.hex) }
        }
        counts.keep(only: Set(machines.map(\.computer.fingerprint.hex)))
        counts.save(to: defaults)
        return counts.total
    }

    /// Writes what widgets, Controls, Siri, Spotlight, and the share
    /// extension show, and reloads the widgets when it changed.
    func publishSummary() {
        let chats = chatItems.map { item in
            WidgetSummary.Chat(
                device: item.computer.device,
                computerId: item.computer.fingerprint.hex,
                computer: item.computer.name,
                pane: item.chat.pane,
                title: item.chat.title,
                workspace: item.workspace.label,
                workspaceId: item.workspace.id,
                agent: item.chat.agent,
                needsYou: item.needsYou,
                working: item.working,
                question: item.question.map { question in
                    WidgetSummary.Question(
                        permission: question.id,
                        title: question.title,
                        options: question.options.prefix(4).map { .init(id: $0.id, name: $0.name, kind: $0.kind) }
                    )
                }
            )
        }
        if WidgetSummary(chats: chats, updated: .now).save() {
            WidgetCenter.shared.reloadTimelines(ofKind: WidgetSummary.kind)
        }
        onSummary?()
    }

    /// Called after each summary, for Spotlight.
    @ObservationIgnored var onSummary: (() -> Void)?
    /// Called when a computer's link comes online, for host pairing.
    @ObservationIgnored var onOnline: ((Computer) -> Void)?

    // MARK: Views

    func openChat(_ key: PaneKey) -> ChatStore {
        chatViewers[key, default: 0] += 1
        if let open = chats[key] { return open }
        let store = ChatStore(key: key, cached: cache.chat(key.device, pane: key.pane))
        chats[key] = store
        let since = store.cursor
        Task { await links[key.device]?.watch(.chat(pane: key.pane), since: since) }
        return store
    }

    func closeChat(_ store: ChatStore) {
        guard chats[store.key] === store, Self.release(store.key, from: &chatViewers) else { return }
        chats[store.key] = nil
        if let transcript = store.transcript {
            cache.save(chat: transcript, cursor: store.cursor, device: store.key.device, pane: store.key.pane)
        }
        Task { await links[store.key.device]?.unwatch(.chat(pane: store.key.pane)) }
    }

    func openTerm(_ key: PaneKey) -> TermStore {
        termViewers[key, default: 0] += 1
        if let open = terms[key] { return open }
        let store = TermStore(key: key)
        terms[key] = store
        Task { await links[key.device]?.watch(.term(pane: key.pane)) }
        return store
    }

    /// Scrolls a terminal's history by `lines`, above zero going back.
    func scrollTerm(_ key: PaneKey, lines: Int) {
        Task { await links[key.device]?.scroll(pane: key.pane, lines: lines) }
    }

    /// Asks the computer to size the terminal to fit this screen while
    /// it is watched; Herdr gives it back to the desktop afterwards.
    func sizeTerm(_ store: TermStore, to size: TermSize) {
        guard terms[store.key] === store, store.size != size else { return }
        store.size = size
        let key = store.key
        Task { await links[key.device]?.watch(.term(pane: key.pane), size: size) }
    }

    func closeTerm(_ store: TermStore) {
        guard terms[store.key] === store, Self.release(store.key, from: &termViewers) else { return }
        terms[store.key] = nil
        Task { await links[store.key.device]?.unwatch(.term(pane: store.key.pane)) }
    }

    /// One view of `key` closed; whether it was the last.
    private static func release(_ key: PaneKey, from viewers: inout [PaneKey: Int]) -> Bool {
        let left = (viewers[key] ?? 1) - 1
        viewers[key] = left > 0 ? left : nil
        return left <= 0
    }

    /// Sends a command to a chat; answers a reason to show when it failed
    /// or the chat refused it. Until the chat shows it, `chat(_:for:)`
    /// draws it as done; a prompt has its own outgoing row instead.
    func send(_ command: ChatCommand, to key: PaneKey) async -> String? {
        let tracked: PendingCommand.ID? = if case .prompt = command { nil } else { pending.start(command, to: key) }
        let failure: String?
        do {
            failure = try await call(key.device) { client, token in
                try await client.command(command, pane: key.pane, bearer: token)
            }
        } catch {
            failure = error.localizedDescription
        }
        if let tracked {
            pending.finished(tracked, failure: failure) { shown($0) }
        }
        return failure
    }

    /// The chat with the changes this phone sent that it does not show yet.
    func chat(_ transcript: Transcript, for key: PaneKey) -> Transcript {
        pending.apply(to: transcript, for: key)
    }

    /// Whether the chat shows what `command` changes: its open transcript,
    /// or for an answer from the home list, the question it lists.
    private func shown(_ command: PendingCommand) -> Bool {
        guard let expected = command.expected else { return true }
        if let transcript = chats[command.key]?.transcript { return expected.shown(in: transcript) }
        guard case let .answer(permission) = expected else { return false }
        let item = machines.first { $0.id == command.key.device }?.home?.workspaces
            .flatMap(\.chats).first { $0.pane == command.key.pane }
        return item?.summary?.question?.id != permission
    }

    /// Calls the daemon of `device` at the address its link is using, over
    /// one session per computer so later calls skip the TLS handshake.
    func call<Answer: Sendable>(
        _ device: String,
        _ body: @Sendable (RemoteClient, String) async throws -> Answer
    ) async throws -> Answer {
        guard let machine = machines.first(where: { $0.id == device }),
              case let .online(origin) = machine.status
        else { throw FleetError.offline(machines.first { $0.id == device }?.computer.name ?? "The computer") }
        let token = try await sessions.token(for: machine.computer, at: origin)
        let client: RemoteClient
        if let known = clients[device], known.origin == origin, known.fingerprint == machine.computer.fingerprint {
            client = known
        } else {
            client = RemoteClient(origin: origin, fingerprint: machine.computer.fingerprint)
            clients[device] = client
        }
        return try await body(client, token)
    }

    /// Closes the pane a chat runs in, which stops its agent in Herdr.
    func closeChatPane(_ key: PaneKey) async throws {
        let pane = key.pane
        _ = try await call(key.device) { client, token in
            try await client.termAction(.closePane, pane: pane, bearer: token)
        }
    }

    /// Names a chat or the terminal tab a pane is in; the next `home` from
    /// the computer carries the name to every list.
    func rename(_ key: PaneKey, to label: String) async throws {
        let pane = key.pane
        try await call(key.device) { client, token in
            try await client.rename(label, pane: pane, bearer: token)
        }
    }

    // MARK: Links

    private func startLink(_ computer: Computer) {
        guard links[computer.device] == nil else { return }
        let link = ComputerLink(
            fingerprint: computer.fingerprint.hex,
            origins: computer.origins,
            io: LiveSocket.io(for: computer, sessions: sessions)
        )
        links[computer.device] = link
        let device = computer.device
        let watching = chats.filter { $0.key.device == device }.map { ($0.key.pane, $0.value.cursor) }
        let terminals = terms.filter { $0.key.device == device }.map { ($0.key.pane, $0.value.size) }
        readers[device] = Task {
            await link.watch(.home)
            for (pane, cursor) in watching { await link.watch(.chat(pane: pane), since: cursor) }
            for (pane, size) in terminals { await link.watch(.term(pane: pane), size: size) }
            await link.start()
            for await update in link.updates {
                self.handle(update, from: device)
            }
        }
    }

    private func stopLink(_ device: String) {
        clients[device] = nil
        readers.removeValue(forKey: device)?.cancel()
        if let link = links.removeValue(forKey: device) { Task { await link.stop() } }
    }

    private func handle(_ update: LinkUpdate, from device: String) {
        guard let index = machines.firstIndex(where: { $0.id == device }) else { return }
        switch update {
        case let .status(status):
            machines[index].status = status
            if case .online = status {} else { machines[index].watching = false }
            if case .revoked = status { stopLink(device) }
            if case .online = status { onOnline?(machines[index].computer) }
        case let .event(.home(home)):
            // The computer's load moves every few seconds; only the header
            // shows it, so the cache, widgets, and Spotlight wait for a
            // change to the workspaces.
            if machines[index].watching, machines[index].home?.sameApartFromLoad(home) == true {
                machines[index].home = home
                machines[index].updated = .now
                return
            }
            // Only a change seen between two live homes is dated now.
            if machines[index].watching, let previous = machines[index].home {
                let before = machines[index].answers
                machines[index].answers.update(from: previous, to: home, now: .now)
                if machines[index].answers != before { cache.save(answers: machines[index].answers, device: device) }
            }
            machines[index].home = home
            machines[index].stale = false
            machines[index].watching = true
            machines[index].updated = .now
            cache.save(home: home, device: device)
            for key in Set(pending.items.filter { $0.key.device == device }.map(\.key)) {
                pending.settle(key) { shown($0) }
            }
            publishSummary()
        case let .event(.error(message)):
            log.error("\(self.machines[index].computer.name, privacy: .public) refused: \(message, privacy: .public)")
        case let .event(.termScreen(screen)):
            terms[PaneKey(device: device, pane: screen.pane)]?.apply(screen)
        case let .event(event):
            guard let pane = event.pane, let store = chats[PaneKey(device: device, pane: pane)] else { return }
            let before = store.diverged
            store.apply(event)
            pending.settle(store.key) { shown($0) }
            if store.diverged, !before {
                // A change that did not fit: start over from a snapshot.
                Task { await links[device]?.watch(.chat(pane: pane), since: nil) }
            }
        }
    }
}

extension LiveEvent {
    /// The chat pane a chat event is about.
    var pane: String? {
        switch self {
        case let .chatSnapshot(snapshot): snapshot.pane
        case let .chatResumed(pane, _, _), let .chatChanges(pane, _), let .chatConnection(pane, _): pane
        default: nil
        }
    }
}
