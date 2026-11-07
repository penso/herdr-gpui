import AppIntents
import SheprdRemoteCore

/// A paired computer, for Siri and Shortcuts.
struct ComputerEntity: AppEntity {
    static let typeDisplayRepresentation: TypeDisplayRepresentation = "Sheprd Computer"
    static let defaultQuery = ComputerQuery()

    /// The CA fingerprint, which `BackgroundCommands` signs in by.
    let id: String
    let name: String

    init(_ computer: Computer) {
        id = computer.fingerprint.description
        name = computer.name
    }

    var displayRepresentation: DisplayRepresentation { DisplayRepresentation(title: "\(name)") }
}

struct ComputerQuery: EntityStringQuery {
    private func all() -> [ComputerEntity] {
        guard !AppLock.isOn else { return [] }
        return ((try? ComputerStore.load()) ?? []).map(ComputerEntity.init)
    }

    func entities(for identifiers: [ComputerEntity.ID]) async throws -> [ComputerEntity] {
        all().filter { identifiers.contains($0.id) }
    }

    func entities(matching string: String) async throws -> [ComputerEntity] {
        all().filter { $0.name.localizedCaseInsensitiveContains(string) }
    }

    func suggestedEntities() async throws -> [ComputerEntity] { all() }
}

/// Words for the app to act on, typed or from Siri. They go through what
/// spoken commands do and stop at the same card: it opens the app, as
/// nothing that reaches an agent runs unseen.
struct TellIntent: AppIntent {
    static let title: LocalizedStringResource = "Tell Sheprd"
    static let description = IntentDescription("Shows what a command would do, to confirm: start an agent, or ask, stop, or open a chat.")
    static let openAppWhenRun = true

    @Parameter(title: "Command", requestValueDialog: "What should I do?") var command: String

    static var parameterSummary: some ParameterSummary { Summary("Tell Sheprd \(\.$command)") }

    @MainActor
    func perform() async throws -> some IntentResult {
        AppModel.current?.show(.talk(saying: command))
        return .result()
    }
}

/// Starts an agent with every detail given, which Siri and Shortcuts can
/// fill in, and runs once confirmed, without opening the app.
struct StartAgentIntent: AppIntent {
    static let title: LocalizedStringResource = "Start an Agent"
    static let description = IntentDescription("Starts an agent in a project on one of your computers, in a new worktree when you name a topic.")
    // A prompt can make an agent do anything: never from a locked phone.
    static let authenticationPolicy = IntentAuthenticationPolicy.requiresAuthentication

    @Parameter(title: "Computer") var computer: ComputerEntity
    @Parameter(title: "Project", requestValueDialog: "Which project?") var project: String
    @Parameter(title: "Agent", requestValueDialog: "Which agent?") var agent: String
    @Parameter(title: "Worktree Topic") var topic: String?
    @Parameter(title: "Prompt", requestValueDialog: "What should it do?") var prompt: String

    static var parameterSummary: some ParameterSummary {
        Summary("Start \(\.$agent) on \(\.$computer) in \(\.$project) to \(\.$prompt)") { \.$topic }
    }

    func perform() async throws -> some IntentResult & ProvidesDialog {
        try AppLockError.check()
        try await requestConfirmation(actionName: .go, dialog: "Start \(agent) on \(computer.name) in \(project)?")
        let target = computer
        let details = "\(agent) \(project)"
        let topic = topic
        let prompt = prompt
        let created = try await BackgroundCommands.createWorkspace(computer: target.id) { options in
            let catalog = VoiceCatalog(
                computers: [VoiceCatalog.Computer(id: target.id, name: target.name, online: true, options: options)],
                chats: []
            )
            var draft = VoiceDraft(text: details)
            draft.intent = .newWorkspace
            draft.computer = target.id
            draft.task = prompt
            draft.topic = topic
            draft.worktree = topic != nil
            draft = VoiceParser.edit(draft, saying: details, catalog: catalog)
            guard case let .workspace(plan) = VoicePlan.resolve(draft, catalog: catalog),
                  draft.project != nil, draft.agent != nil,
                  let request = plan.request(start: nil)
            else { throw StartAgentError.notFound }
            return request
        }
        guard prompt.isEmpty || created.prompted else {
            return .result(dialog: "The agent started, but the prompt was not sent. Open Sheprd to send it.")
        }
        return .result(dialog: "\(agent) is on it.")
    }
}

enum StartAgentError: Error, CustomLocalizedStringResourceConvertible {
    case notFound

    var localizedStringResource: LocalizedStringResource {
        "That computer does not offer that project or agent."
    }
}
