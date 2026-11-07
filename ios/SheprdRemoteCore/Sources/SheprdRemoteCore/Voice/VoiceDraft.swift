import Foundation

/// What a sentence asks for.
public enum VoiceIntent: String, CaseIterable, Hashable, Sendable {
    case answerChat = "answer_chat"
    case askChat = "ask_chat"
    case needsMe = "needs_me"
    case newWorkspace = "new_workspace"
    case none
    case openChat = "open_chat"
    case stopChat = "stop_chat"
}

/// The kinds of thing recognized in a sentence, to color it as it is said.
public enum VoiceSlot: Hashable, Sendable {
    case agent, chat, computer, project, task, topic
}

/// A stretch of the sentence that was understood.
public struct VoiceMark: Hashable, Sendable {
    public var range: Range<String.Index>
    public var slot: VoiceSlot
}

/// How a permission question is answered by voice.
public enum VoiceAnswer: Hashable, Sendable {
    case allow
    case deny
    /// An option named in the sentence, for a question with its own.
    case saying(String)
}

/// What was understood so far: only what was said, with nothing filled in.
/// `VoicePlan` fills in the rest.
public struct VoiceDraft: Hashable, Sendable {
    public var text = ""
    public var intent = VoiceIntent.none
    /// Rules recognized the sentence's shape, so the model is not needed.
    public var sure = false
    /// `VoiceCatalog.Computer.id`.
    public var computer: String?
    public var leastBusy = false
    public var project: String?
    /// `AgentChoice.id`.
    public var agent: String?
    /// `VoiceCatalog.Chat.id`.
    public var chat: String?
    public var answer: VoiceAnswer?
    public var worktree = false
    /// A branch said or typed outright, which `topic` then does not name.
    public var branch: String?
    public var topic: String?
    public var task: String?
    public var marks: [VoiceMark] = []

    public init(text: String = "") {
        self.text = text
    }
}

/// A model's reading of a sentence, as plain words. None of it is believed
/// until `VoiceDraft.adopting` has checked it against the sentence.
public struct VoiceReading: Hashable, Sendable {
    public var action: String
    public var computer: String?
    public var project: String?
    public var agent: String?
    public var chat: String?
    public var topic: String?
    public var task: String?

    public init(action: String, computer: String? = nil, project: String? = nil, agent: String? = nil, chat: String? = nil, topic: String? = nil, task: String? = nil) {
        self.action = action
        self.computer = computer
        self.project = project
        self.agent = agent
        self.chat = chat
        self.topic = topic
        self.task = task
    }
}

extension VoiceDraft {
    /// Takes from `reading` what the rules left open, and only what the
    /// sentence bears out. A small model readily names a computer nobody
    /// said, so a name counts only if it was heard, and a task only if its
    /// words are the speaker's own: the prompt must not be a paraphrase.
    public func adopting(_ reading: VoiceReading, catalog: VoiceCatalog) -> VoiceDraft {
        var draft = self
        let words = VoiceNames.words(text)
        func heard(_ name: String?, partly: Bool = false) -> Bool {
            guard let name else { return false }
            return VoiceNames.find(name, in: words, partly: partly) != nil
        }
        if draft.computer == nil, let name = reading.computer, heard(name, partly: true) {
            draft.computer = catalog.computers.first { $0.name == name }?.id
        }
        if draft.project == nil, heard(reading.project) { draft.project = reading.project }
        if draft.agent == nil, heard(reading.agent) { draft.agent = reading.agent }
        if draft.chat == nil, let title = reading.chat, heard(title, partly: true) {
            draft.chat = catalog.chats.first { $0.title == title }?.id
        }
        if draft.topic == nil, let topic = reading.topic, contains(topic) { draft.topic = topic }
        if draft.task == nil, let task = reading.task, contains(task) { draft.task = task }
        guard !draft.sure, let intent = VoiceIntent(rawValue: reading.action) else { return draft }
        switch intent {
        case .stopChat, .answerChat:
            // These need their own verb, which the rules would have read.
            // The model took "what's the status of" for answering a chat.
            draft.intent = .none
        case .askChat:
            draft.intent = draft.chat == nil || draft.task == nil ? .none : intent
        case .openChat:
            draft.intent = draft.chat == nil ? .none : intent
        case .newWorkspace:
            // Nothing here was asked of an agent or a computer.
            let named = draft.agent != nil || draft.computer != nil || draft.project != nil || draft.worktree
            draft.intent = named ? intent : .none
        case .needsMe, .none:
            draft.intent = intent
        }
        return draft
    }

    /// Whether `words` are the speaker's own, in order, give or take case
    /// and punctuation.
    private func contains(_ quote: String) -> Bool {
        let said = VoiceNames.words(text).map(\.text)
        let quoted = VoiceNames.words(quote).map(\.text)
        guard !quoted.isEmpty, quoted.count <= said.count else { return false }
        return said.indices.contains { start in
            start + quoted.count <= said.count && Array(said[start..<(start + quoted.count)]) == quoted
        }
    }
}
