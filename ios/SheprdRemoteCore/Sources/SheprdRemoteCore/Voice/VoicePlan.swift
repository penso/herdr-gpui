import Foundation

/// What a command will do once confirmed: every detail filled in and
/// checked against the fleet, so the card shows exactly what would run.
public enum VoicePlan: Hashable, Sendable {
    case chat(Chat)
    /// One command on several computers.
    case fanOut(FanOut)
    case needsMe
    /// Not a command this app knows.
    case unknown
    case workspace(Workspace)

    /// Why a detail has the value it has, shown beside it.
    public enum Source: Hashable, Sendable {
        case leastBusy
        /// The last one used, as nobody said.
        case recent
        case said
    }

    public struct Workspace: Hashable, Sendable {
        public var computer: VoiceCatalog.Computer?
        public var computerSource = Source.recent
        public var project: VoiceCatalog.Project?
        public var projectSource = Source.recent
        /// Empty opens the checkout itself, with no worktree.
        public var branch = ""
        public var agent: AgentChoice?
        public var agentSource = Source.recent
        public var prompt = ""
        /// What must be settled before it can start.
        public var question: Question?

        /// The request to send, once nothing is left to settle.
        public func request(start: ChatStart?) -> NewWorkspaceRequest? {
            guard question == nil, let project, let agent else { return nil }
            return NewWorkspaceRequest(cwd: project.folder, branch: branch, agent: agent.id, prompt: prompt, start: start)
        }
    }

    public struct FanOut: Hashable, Sendable {
        public var plans: [Workspace]
        /// Computers it cannot run on, by name: offline, or without the
        /// project or agent.
        public var skipped: [String]
    }

    /// One thing the app has to ask.
    public enum Question: Hashable, Sendable {
        /// `named` was asked for by name and is not on the computer.
        case agent(on: String, named: String?)
        case computer
        /// A computer was named that is not connected.
        case offline(String)
        /// `name` was asked for by name and is not on the computer.
        case project(on: String, named: String?, choices: [VoiceCatalog.Project])
    }

    public struct Chat: Hashable, Sendable {
        public var chat: VoiceCatalog.Chat
        public var action: Action

        public enum Action: Hashable, Sendable {
            case answer(PermissionOption, PermissionId)
            case ask(String)
            case open
            case stop
            /// A question was meant, but none waits, or no option fits.
            case unanswerable
        }
    }

    /// Whether it changes anything, so needs a yes. Showing things does
    /// not; anything that reaches an agent does.
    public var needsConfirmation: Bool {
        switch self {
        case .needsMe, .unknown: false
        case let .chat(chat): chat.action != .open
        case .fanOut, .workspace: true
        }
    }

    /// Whether a spoken yes may run it. Only a new worktree: it touches no
    /// existing checkout, and an unwanted one is simply deleted.
    public var confirmableByVoice: Bool {
        if case let .workspace(plan) = self { plan.question == nil && !plan.branch.isEmpty } else { false }
    }

    /// The card read aloud, for someone not looking at it.
    public var readBack: String? {
        guard case let .workspace(plan) = self, let agent = plan.agent, let computer = plan.computer, let project = plan.project else { return nil }
        let task = plan.prompt.isEmpty ? "" : " to \(plan.prompt)"
        return "\(agent.name) on \(computer.name), in \(project.name)\(task). Say yes to start."
    }

    /// Fills in `draft`, or says what is missing.
    public static func resolve(_ draft: VoiceDraft, catalog: VoiceCatalog) -> VoicePlan {
        switch draft.intent {
        case .needsMe: return .needsMe
        case .none: return .unknown
        case .newWorkspace:
            if let fanOut = draft.fanOut { return .fanOut(fanOutPlan(fanOut, draft, catalog: catalog)) }
            return .workspace(workspace(draft, catalog: catalog))
        case .askChat, .stopChat, .openChat, .answerChat:
            guard let chat = catalog.chats.first(where: { $0.id == draft.chat }) else { return .unknown }
            return .chat(Chat(chat: chat, action: action(draft, chat: chat)))
        }
    }

    /// The same sentence as a task for an agent, for what the app itself
    /// cannot do: one on a computer can look around and use the `herdr` CLI.
    public static func handOff(_ draft: VoiceDraft, catalog: VoiceCatalog) -> VoicePlan {
        var plan = workspace(draft, catalog: catalog)
        plan.prompt = draft.task ?? draft.text
        plan.branch = draft.branch ?? ""
        return .workspace(plan)
    }

    private static func fanOutPlan(_ fanOut: VoiceFanOut, _ draft: VoiceDraft, catalog: VoiceCatalog) -> FanOut {
        let targets: [VoiceCatalog.Computer] = switch fanOut {
        case .every: catalog.computers
        case let .computers(ids): ids.compactMap { id in catalog.computers.first { $0.id == id } }
        }
        var result = FanOut(plans: [], skipped: [])
        for computer in targets {
            var one = draft
            one.fanOut = nil
            one.computer = computer.id
            let plan = workspace(one, catalog: catalog)
            if plan.question == nil { result.plans.append(plan) } else { result.skipped.append(computer.name) }
        }
        return result
    }

    private static func action(_ draft: VoiceDraft, chat: VoiceCatalog.Chat) -> Chat.Action {
        switch draft.intent {
        case .stopChat: return .stop
        case .openChat: return .open
        case .askChat: return draft.task.map(Chat.Action.ask) ?? .open
        default: break
        }
        guard let question = chat.question, let option = option(draft.answer, in: question) else { return .unanswerable }
        return .answer(option, question.id)
    }

    /// Only one-time answers: "always" outlives the question, which is
    /// more than a spoken word should decide.
    private static func option(_ answer: VoiceAnswer?, in question: Permission) -> PermissionOption? {
        switch answer {
        case .allow: return question.options.first { $0.kind == .allowOnce }
        case .deny: return question.options.first { $0.kind == .rejectOnce }
        case let .saying(text):
            let words = VoiceNames.words(text)
            return question.options
                .filter { $0.kind != .allowAlways && $0.kind != .rejectAlways }
                .compactMap { option in VoiceNames.find(option.name, in: words, partly: true).map { (option, $0.score) } }
                .max { $0.1 < $1.1 }?.0
        case nil: return nil
        }
    }

    private static func workspace(_ draft: VoiceDraft, catalog: VoiceCatalog) -> Workspace {
        var plan = Workspace()
        plan.prompt = draft.task ?? ""
        if draft.worktree {
            plan.branch = draft.branch ?? draft.topic.flatMap(VoiceBranch.name) ?? NewWorkspaceRequest.proposedBranch()
        }
        chooseComputer(&plan, draft, catalog)
        guard let computer = plan.computer, plan.question == nil else { return plan }
        chooseProject(&plan, draft, on: computer)
        chooseAgent(&plan, draft, on: computer, catalog)
        return plan
    }

    private static func chooseComputer(_ plan: inout Workspace, _ draft: VoiceDraft, _ catalog: VoiceCatalog) {
        if let named = catalog.computers.first(where: { $0.id == draft.computer }) {
            plan.computer = named
            plan.computerSource = .said
            if !named.online { plan.question = .offline(named.name) }
            return
        }
        let online = catalog.computers.filter(\.online)
        let having = draft.project.map { name in online.filter { $0.projects.contains { $0.name == name } } } ?? []
        let pool = having.isEmpty ? online : having
        if draft.leastBusy || (draft.project != nil && pool.count > 1) {
            let candidates = pool.compactMap { computer in
                computer.home.map { Home.Candidate(id: computer.id, label: computer.name, home: $0) }
            }
            if let best = Home.leastBusy(candidates, current: catalog.recentDevice, repository: draft.project).first {
                plan.computer = pool.first { $0.id == best.id }
                plan.computerSource = .leastBusy
                return
            }
        }
        if let computer = pool.first(where: { $0.id == catalog.recentDevice }) ?? pool.first {
            plan.computer = computer
            plan.computerSource = .recent
        } else {
            plan.question = .computer
        }
    }

    private static func chooseProject(_ plan: inout Workspace, _ draft: VoiceDraft, on computer: VoiceCatalog.Computer) {
        let projects = computer.projects
        if let name = draft.project {
            plan.project = projects.first { $0.name == name }
            plan.projectSource = .said
            if plan.project == nil {
                plan.question = .project(on: computer.name, named: name, choices: Array(projects.prefix(4)))
            }
            return
        }
        plan.project = projects.first
        plan.projectSource = .recent
        if plan.project == nil { plan.question = .project(on: computer.name, named: nil, choices: []) }
    }

    private static func chooseAgent(_ plan: inout Workspace, _ draft: VoiceDraft, on computer: VoiceCatalog.Computer, _ catalog: VoiceCatalog) {
        let agents = computer.agents
        if let id = draft.agent {
            plan.agent = agents.first { $0.id == id }
            plan.agentSource = .said
        } else {
            plan.agent = agents.first { $0.id == catalog.recentAgent } ?? agents.first
            plan.agentSource = .recent
        }
        if plan.agent == nil, plan.question == nil { plan.question = .agent(on: computer.name, named: draft.agent) }
    }
}

extension VoicePlan.Question {
    /// What the app says and shows to ask it.
    public var words: String {
        switch self {
        case let .agent(on, named?): "\(named.capitalized) is not on \(on). Which agent?"
        case let .agent(on, nil): "Which agent on \(on)?"
        case .computer: "None of your computers is connected."
        case let .offline(name): "\(name) is not connected."
        case let .project(on, named?, _): "\(named) is not on \(on). Which project?"
        case let .project(on, nil, _): "Which project on \(on)?"
        }
    }

    /// Whether a spoken reply can settle it.
    public var answerable: Bool {
        switch self {
        case .agent, .project: true
        case .computer, .offline: false
        }
    }
}
