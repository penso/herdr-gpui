import Foundation
import Observation
import SheprdRemoteCore

/// One spoken command, from the microphone opening to the result. Rules
/// read the words as they come, the on-device model takes what they could
/// not place, and nothing that reaches an agent runs without a tap.
@MainActor
@Observable
final class VoiceSession: Identifiable {
    enum Phase: Equatable {
        case preparing
        case listening
        case thinking
        /// A plan, or a question about one, waiting on the person.
        case ready
        case running
        /// Words back with nothing to confirm, such as what needs you.
        case answered(String)
        case done(Done)
        case failed(String)
    }

    struct Waiting: Identifiable, Equatable {
        var title: String
        var computer: String
        var route: Route?
        var id: String { "\(computer)/\(title)" }
    }

    struct Done: Equatable {
        var title: String
        var detail: String
        var route: Route?
    }

    /// How long the microphone stays open with nothing said.
    private static let patience = Duration.seconds(8)
    /// How long a slow computer may hold up the plan. It then shows what
    /// the other computers offer.
    private static let optionsWait = Duration.seconds(3)

    let id = UUID()
    let listener = VoiceListener()
    private(set) var phase = Phase.preparing
    private(set) var draft = VoiceDraft()
    private(set) var plan = VoicePlan.unknown
    private(set) var catalog: VoiceCatalog
    /// The sentence goes to an agent as it was said.
    private(set) var handedOff = false
    /// What a reply is changing, while one is being heard.
    private var replyingTo: VoiceDraft?
    /// The words of that reply so far.
    private(set) var reply = ""
    /// A spoken yes would start the plan on the card.
    private(set) var awaitingYes = false
    /// What needs the person, after "what needs me".
    private(set) var waiting: [Waiting] = []
    /// When Start was pressed, while it runs.
    private(set) var runningSince: Date?
    private var silence: Task<Void, Never>?
    private var loading: Task<Void, Never>?
    private let speaker = VoiceSpeaker()
    private unowned let model: AppModel

    init(model: AppModel) {
        self.model = model
        catalog = VoiceSession.catalog(of: model)
    }

    /// The fleet, and the last chat though its home has not listed it yet.
    private static func catalog(of model: AppModel) -> VoiceCatalog {
        var catalog = model.voiceCatalog()
        if let chat = model.voiceContext.chat, !catalog.chats.contains(where: { $0.id == chat.id }) {
            catalog.chats.append(chat)
        }
        return catalog
    }

    /// Whether the microphone is open or about to be.
    var listening: Bool { phase == .listening || phase == .preparing }
    /// Whether the first sentence is under way, when rows show only what
    /// was said. A reply changes a card that is already filled in.
    var firstSentence: Bool { listening && replyingTo == nil }

    /// Starts with `text` already said, or by listening.
    func begin(saying text: String? = nil) async {
        loading = Task { [weak self] in await self?.loadOptions() }
        if let text {
            heard(text)
            await finish()
        } else {
            await listen()
        }
    }

    /// The person is done speaking, or the pause said so.
    func stopListening() {
        guard phase == .listening else { return }
        Task { await finish() }
    }

    /// Hears a correction or an answer to the plan on the card.
    func listenAgain(forYes: Bool = false) {
        speaker.stop()
        awaitingYes = forYes
        // With no plan there is nothing to correct: it starts over.
        replyingTo = plan == .unknown ? nil : draft
        reply = ""
        Task { await listen() }
    }

    func cancel() {
        silence?.cancel()
        loading?.cancel()
        speaker.stop()
        Task { await listener.stop() }
    }

    // MARK: Changing the plan by hand

    func choose(computer: VoiceCatalog.Computer) { change { $0.computer = computer.id; $0.leastBusy = false } }
    func choose(project: VoiceCatalog.Project) { change { $0.project = project.name } }
    func choose(agent: AgentChoice) { change { $0.agent = agent.id } }
    func edit(prompt: String) { change { $0.task = prompt } }

    /// An empty branch means no worktree.
    func edit(branch: String) {
        change {
            $0.branch = branch
            $0.worktree = !branch.isEmpty
        }
    }

    /// Gives what the app could not do to an agent, on the card first.
    func handOff() {
        handedOff = true
        draft.intent = .newWorkspace
        draft.task = draft.text
        replan()
        phase = .ready
    }

    private func change(_ edit: (inout VoiceDraft) -> Void) {
        edit(&draft)
        replan()
    }

    private func replan() {
        catalog = Self.catalog(of: model)
        plan = handedOff ? VoicePlan.handOff(draft, catalog: catalog) : VoicePlan.resolve(draft, catalog: catalog)
    }

    // MARK: Hearing

    private func listen() async {
        phase = .preparing
        do {
            try await listener.start(names: catalog.vocabulary) { [weak self] text in self?.heard(text) }
            phase = .listening
            waitForWords(Self.patience)
        } catch {
            phase = .failed(error.localizedDescription)
        }
    }

    private func waitForWords(_ wait: Duration) {
        silence?.cancel()
        silence = Task { [weak self] in
            try? await Task.sleep(for: wait)
            guard !Task.isCancelled else { return }
            self?.stopListening()
        }
    }

    private func heard(_ text: String) {
        catalog = Self.catalog(of: model)
        if let base = replyingTo {
            draft = VoiceParser.edit(base, saying: text, catalog: catalog)
            reply = text
        } else {
            draft = VoiceParser.parse(text, catalog: catalog, context: model.voiceContext, english: VoiceSettings.english)
        }
        replan()
        waitForWords(VoiceEndpoint.pause(after: draft, replying: replyingTo != nil))
    }

    private func finish() async {
        silence?.cancel()
        phase = .thinking
        await listener.stop()
        await waitForOptions()
        catalog = Self.catalog(of: model)
        if replyingTo == nil, !draft.text.isEmpty, !draft.sure,
           let reading = await VoiceModel.read(draft.text, catalog: catalog) {
            draft = draft.adopting(reading, catalog: catalog)
        }
        let answer = reply
        replyingTo = nil
        reply = ""
        replan()
        tidyBranch()
        if awaitingYes {
            awaitingYes = false
            switch VoiceReply(answer) {
            case .yes where plan.confirmableByVoice:
                phase = .ready
                await confirm()
                return
            case .no:
                model.voice = nil
                return
            case .yes, .other:
                // Silence leaves the card for a tap; words corrected it.
                guard !answer.isEmpty else {
                    phase = .ready
                    return
                }
            }
        }
        await act()
    }

    /// Puts right a branch named after a topic the transcriber garbled,
    /// after the card is up, so it never holds the card back.
    private func tidyBranch() {
        guard draft.worktree, draft.branch == nil, let topic = draft.topic, let task = draft.task,
              VoiceBranch.doubtful(topic: topic, task: task) else { return }
        Task { [weak self] in
            guard let words = await VoiceModel.tidy(topic: topic, task: task),
                  let self, self.phase == .ready, self.draft.branch == nil,
                  let branch = VoiceBranch.name(for: words, topic: topic) else { return }
            self.draft.branch = branch
            self.replan()
        }
    }

    /// What comes after hearing: run what only shows something, and put the
    /// rest on the card.
    private func act() async {
        switch plan {
        case .needsMe:
            waiting = waitingNow()
            let words = Self.waitingWords(waiting)
            phase = .answered(words)
            await speaker.say(words)
        case let .chat(chat) where chat.action == .open:
            model.voiceContext.chat = chat.chat
            model.pendingRoute = route(to: chat.chat)
            model.voice = nil
        case let .workspace(workspace):
            phase = .ready
            if let question = workspace.question {
                await speaker.say(question.words, always: VoiceSettings.confirms)
                if question.answerable, phase == .ready { listenAgain() }
            } else if VoiceSettings.confirms, plan.confirmableByVoice, let words = plan.readBack {
                await speaker.say(words, always: true)
                if phase == .ready { listenAgain(forYes: true) }
            }
        default:
            phase = .ready
        }
    }

    private func route(to chat: VoiceCatalog.Chat) -> Route {
        let key = PaneKey(device: chat.device, pane: chat.pane)
        return chat.terminal ? .term(key, title: chat.title) : .chat(key, title: chat.title)
    }

    private func waitingNow() -> [Waiting] {
        let chats = model.fleet.chatItems.filter(\.needsYou).map { item in
            Waiting(title: item.chat.title, computer: model.displayName(item.computer), route: .chat(item.key, title: item.chat.title))
        }
        let terminals = model.fleet.agentTerminals.filter(\.needsYou).map { item in
            Waiting(title: item.title, computer: model.displayName(item.computer), route: item.route)
        }
        return chats + terminals
    }

    private static func waitingWords(_ waiting: [Waiting]) -> String {
        let titles = waiting.prefix(3).map(\.title).joined(separator: ", ")
        return switch waiting.count {
        case 0: "Nothing needs you."
        case 1: "\(titles) needs you."
        default: "\(waiting.count) chats need you: \(titles)."
        }
    }

    private func loadOptions() async {
        let online = model.fleet.machines.filter { if case .online = $0.status { true } else { false } }
        let loads = online.map { machine in
            Task { [weak self] in await self?.loadOptions(of: machine.id) }
        }
        for load in loads { await load.value }
    }

    private func loadOptions(of device: String) async {
        let options = try? await model.fleet.call(device) { client, token in
            try await client.newOptions(bearer: token)
        }
        if let options { model.voiceOptions[device] = options }
    }

    /// Whether any computer's link is up.
    private var anyOnline: Bool {
        model.fleet.machines.contains { if case .online = $0.status { true } else { false } }
    }

    private func waitForOptions() async {
        // From the Action button or Siri the app may have only just
        // launched, and no computer is reachable until a link comes up.
        let clock = ContinuousClock()
        let deadline = clock.now.advanced(by: Self.optionsWait)
        while !anyOnline, clock.now < deadline {
            try? await Task.sleep(for: .milliseconds(100))
        }
        // The links came up while the person spoke, after the first load
        // found nothing online.
        let missing = model.fleet.machines.contains { machine in
            if case .online = machine.status { model.voiceOptions[machine.id] == nil } else { false }
        }
        if missing { loading = Task { [weak self] in await self?.loadOptions() } }
        guard let loading else { return }
        await withTaskGroup(of: Void.self) { group in
            group.addTask { await loading.value }
            group.addTask { try? await Task.sleep(for: Self.optionsWait) }
            await group.next()
            group.cancelAll()
        }
    }

    // MARK: Running

    /// Runs the plan on the card, once the person has pressed Start. It
    /// goes on if the sheet is closed; `failed` then uses an alert.
    func confirm() async {
        guard plan.needsConfirmation, phase == .ready else { return }
        speaker.stop()
        phase = .running
        runningSince = Date()
        defer { runningSince = nil }
        model.voiceContext.draft = draft
        switch plan {
        case let .workspace(workspace): await start(workspace)
        case let .fanOut(fanOut): await start(fanOut)
        case let .chat(chat): await run(chat)
        case .needsMe, .unknown: break
        }
    }

    private func failed(_ reason: String) {
        phase = .failed(reason)
        if model.voice !== self { model.notice = reason }
    }

    private struct Launch {
        var computer: VoiceCatalog.Computer
        var agent: AgentChoice
        var request: NewWorkspaceRequest
        var start: ChatStart?
        var created: Created

        var clean: Bool {
            (request.prompt.isEmpty || created.prompted) && (start == nil || created.started == true)
        }
    }

    private func launch(_ plan: VoicePlan.Workspace) async throws -> Launch? {
        // Auto-accepting stays a choice made on a screen: a misheard
        // sentence must not run tools nobody saw.
        var start = plan.agent.flatMap { model.startChoice(for: $0.name).start }
        start?.autoAccept = false
        guard let computer = plan.computer, let agent = plan.agent, let request = plan.request(start: start) else { return nil }
        let created = try await model.fleet.call(computer.id) { client, token in
            try await client.createWorkspace(request, bearer: token)
        }
        return Launch(computer: computer, agent: agent, request: request, start: start, created: created)
    }

    private func start(_ plan: VoicePlan.Workspace) async {
        do {
            guard let launch = try await launch(plan) else {
                phase = .ready
                return
            }
            model.recentAgent = launch.agent.id
            model.recentDevice = launch.computer.id
            guard launch.clean, let pane = launch.created.pane else {
                // The form's path keeps the prompt and says what happened.
                model.voice = nil
                model.opened(launch.created, prompt: launch.request.prompt, start: launch.start, device: launch.computer.id, title: launch.agent.name)
                return
            }
            let chat = VoiceCatalog.Chat(device: launch.computer.id, pane: pane, title: launch.agent.name, computer: launch.computer.name)
            model.voiceContext.chat = chat
            let place = plan.project.map { "\($0.name) on \(launch.computer.name)" } ?? launch.computer.name
            let done = Done(
                title: "\(launch.agent.name) is on it",
                detail: plan.branch.isEmpty ? place : "\(place), branch \(plan.branch)",
                route: route(to: chat)
            )
            phase = .done(done)
            await speaker.say(done.title)
        } catch {
            failed(error.localizedDescription)
        }
    }

    /// Every computer at once. One failing does not stop the others, and
    /// the result names each that did not fully start.
    private func start(_ plan: VoicePlan.FanOut) async {
        let runs = plan.plans.map { one in
            (name: one.computer?.name ?? "", task: Task { [weak self] in try await self?.launch(one) })
        }
        var started = 0
        var problems: [String] = []
        for run in runs {
            do {
                if let launch = try await run.task.value, launch.clean { started += 1 } else { problems.append("\(run.name): prompt not sent") }
            } catch {
                problems.append("\(run.name): \(error.localizedDescription)")
            }
        }
        guard started > 0 else {
            failed(problems.joined(separator: "\n"))
            return
        }
        let title = "Started on \(started) computer\(started == 1 ? "" : "s")"
        phase = .done(Done(title: title, detail: (problems + plan.skipped.map { "\($0): skipped" }).joined(separator: "\n"), route: nil))
        await speaker.say(title)
    }

    private func run(_ plan: VoicePlan.Chat) async {
        model.voiceContext.chat = plan.chat
        let result: String
        let failure: String?
        switch plan.action {
        case let .ask(text):
            result = "Sent to \(plan.chat.title)"
            failure = plan.chat.terminal ? await type(text, into: plan.chat) : await send(.prompt(text: text), to: plan.chat)
        case .stop:
            result = "Stopped \(plan.chat.title)"
            failure = plan.chat.terminal ? await press("esc", in: plan.chat) : await send(.cancel, to: plan.chat)
        case let .answer(option, permission):
            result = "Answered \(plan.chat.title)"
            failure = await send(.answer(permission: permission, option: option.id), to: plan.chat)
        case .open, .unanswerable:
            phase = .ready
            return
        }
        if let failure {
            failed(failure)
            return
        }
        phase = .done(Done(title: result, detail: plan.chat.computer, route: route(to: plan.chat)))
        await speaker.say(result)
    }

    private func send(_ command: ChatCommand, to chat: VoiceCatalog.Chat) async -> String? {
        await model.fleet.send(command, to: PaneKey(device: chat.device, pane: chat.pane))
    }

    /// A terminal has no prompt to send to: the words are pasted, as a
    /// person would, then entered. The gap lets the agent take the paste.
    private func type(_ text: String, into chat: VoiceCatalog.Chat) async -> String? {
        do {
            try await model.fleet.call(chat.device) { client, token in
                try await client.termPaste(text, pane: chat.pane, bearer: token)
                try await Task.sleep(for: .milliseconds(200))
                try await client.termKeys(["enter"], pane: chat.pane, bearer: token)
            }
            return nil
        } catch {
            return error.localizedDescription
        }
    }

    /// The key agents take to stop a turn without quitting.
    private func press(_ key: String, in chat: VoiceCatalog.Chat) async -> String? {
        do {
            try await model.fleet.call(chat.device) { client, token in
                try await client.termKeys([key], pane: chat.pane, bearer: token)
            }
            return nil
        } catch {
            return error.localizedDescription
        }
    }
}
