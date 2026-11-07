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

    struct Done: Equatable {
        var title: String
        var detail: String
        var route: Route?
    }

    /// Words stop this long before the command counts as finished.
    private static let pause = Duration.seconds(3)
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
    private var silence: Task<Void, Never>?
    private var loading: Task<Void, Never>?
    private let speaker = VoiceSpeaker()
    private unowned let model: AppModel

    init(model: AppModel) {
        self.model = model
        catalog = model.voiceCatalog()
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
    func listenAgain() {
        speaker.stop()
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
        catalog = model.voiceCatalog()
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
        catalog = model.voiceCatalog()
        if let base = replyingTo {
            draft = VoiceParser.edit(base, saying: text, catalog: catalog)
            reply = text
        } else {
            draft = VoiceParser.parse(text, catalog: catalog)
        }
        replan()
        waitForWords(Self.pause)
    }

    private func finish() async {
        silence?.cancel()
        phase = .thinking
        await listener.stop()
        await waitForOptions()
        catalog = model.voiceCatalog()
        if replyingTo == nil, !draft.text.isEmpty, !draft.sure,
           let reading = await VoiceModel.read(draft.text, catalog: catalog) {
            draft = draft.adopting(reading, catalog: catalog)
        }
        replyingTo = nil
        reply = ""
        replan()
        await act()
    }

    /// What comes after hearing: run what only shows something, and put the
    /// rest on the card.
    private func act() async {
        switch plan {
        case .needsMe:
            let words = VoiceSession.waitingWords(model.fleet.chatItems.filter(\.needsYou))
            phase = .answered(words)
            await speaker.say(words)
        case let .chat(chat) where chat.action == .open:
            model.pendingRoute = .chat(PaneKey(device: chat.chat.device, pane: chat.chat.pane), title: chat.chat.title)
            model.voice = nil
        case let .workspace(workspace):
            phase = .ready
            guard let question = workspace.question else { return }
            await speaker.say(question.words)
            if question.answerable, phase == .ready { listenAgain() }
        default:
            phase = .ready
        }
    }

    private static func waitingWords(_ waiting: [ChatItem]) -> String {
        let titles = waiting.prefix(3).map(\.chat.title).joined(separator: ", ")
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

    /// Runs the plan on the card, once the person has pressed Start.
    func confirm() async {
        guard plan.needsConfirmation, phase == .ready else { return }
        speaker.stop()
        phase = .running
        switch plan {
        case let .workspace(workspace): await start(workspace)
        case let .chat(chat): await run(chat)
        case .needsMe, .unknown: break
        }
    }

    private func start(_ plan: VoicePlan.Workspace) async {
        // Auto-accepting stays a choice made on a screen: a misheard
        // sentence must not run tools nobody saw.
        var start = plan.agent.flatMap { model.startChoice(for: $0.name).start }
        start?.autoAccept = false
        guard let computer = plan.computer, let agent = plan.agent, let request = plan.request(start: start) else {
            phase = .ready
            return
        }
        do {
            let created = try await model.fleet.call(computer.id) { client, token in
                try await client.createWorkspace(request, bearer: token)
            }
            model.recentAgent = agent.id
            model.recentDevice = computer.id
            let clean = (request.prompt.isEmpty || created.prompted) && (start == nil || created.started == true)
            guard clean, let pane = created.pane else {
                // The form's path keeps the prompt and says what happened.
                model.voice = nil
                model.opened(created, prompt: request.prompt, start: start, device: computer.id, title: agent.name)
                return
            }
            let place = plan.project.map { "\($0.name) on \(computer.name)" } ?? computer.name
            let done = Done(
                title: "\(agent.name) is on it",
                detail: plan.branch.isEmpty ? place : "\(place), branch \(plan.branch)",
                route: .chat(PaneKey(device: computer.id, pane: pane), title: agent.name)
            )
            phase = .done(done)
            await speaker.say(done.title)
        } catch {
            phase = .failed(error.localizedDescription)
        }
    }

    private func run(_ plan: VoicePlan.Chat) async {
        let key = PaneKey(device: plan.chat.device, pane: plan.chat.pane)
        let command: ChatCommand
        let result: String
        switch plan.action {
        case let .ask(text):
            command = .prompt(text: text)
            result = "Sent to \(plan.chat.title)"
        case .stop:
            command = .cancel
            result = "Stopped \(plan.chat.title)"
        case let .answer(option, permission):
            command = .answer(permission: permission, option: option.id)
            result = "Answered \(plan.chat.title)"
        case .open, .unanswerable:
            phase = .ready
            return
        }
        if let failure = await model.fleet.send(command, to: key) {
            phase = .failed(failure)
            return
        }
        let done = Done(title: result, detail: plan.chat.computer, route: .chat(key, title: plan.chat.title))
        phase = .done(done)
        await speaker.say(result)
    }
}
