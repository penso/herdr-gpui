import Foundation

/// What the last command was about, so the next can say "it" or "the same
/// thing on the mini".
public struct VoiceContext: Hashable, Sendable {
    public var draft: VoiceDraft?
    public var chat: VoiceCatalog.Chat?

    public init(draft: VoiceDraft? = nil, chat: VoiceCatalog.Chat? = nil) {
        self.draft = draft
        self.chat = chat
    }
}

extension VoiceParser {
    private static let sameOpeners = [
        ["same", "thing"], ["same", "on"], ["same", "in"], ["same", "with"], ["do", "the", "same"],
        ["do", "that", "again"], ["do", "it", "again"], ["again", "on"], ["again", "with"], ["repeat", "that"],
    ]
    private static let pronouns = [["it"], ["that"], ["this"], ["that", "one"], ["this", "one"], ["the", "last", "one"]]
    private static let everyWords: Set<String> = ["every", "all", "each"]
    private static let computerNouns: Set<String> = ["host", "hosts", "computer", "computers", "machine", "machines"]

    /// The last command again, with what this sentence changes.
    static func repeating(_ context: VoiceContext?, saying text: String, words: [VoiceWord], catalog: VoiceCatalog) -> VoiceDraft? {
        guard let last = context?.draft, last.intent == .newWorkspace else { return nil }
        let opens = sameOpeners.contains { opener in
            opener.count <= words.count && zip(opener, words).allSatisfy { $0 == $1.text }
        }
        guard opens else { return nil }
        var draft = edit(last, saying: text, catalog: catalog)
        draft.text = text
        draft.marks = []
        draft.sure = true
        return draft
    }

    /// "Stop it", "open that one", "tell it to carry on": the last chat.
    static func readPronoun(_ words: [VoiceWord], into draft: inout VoiceDraft, context: VoiceContext?) -> Bool {
        guard let chat = context?.chat, let first = words.first?.text else { return false }
        let intent: VoiceIntent
        switch first {
        case "stop", "cancel", "kill", "abort", "halt": intent = .stopChat
        case "open", "show": intent = .openChat
        case "tell", "ask": intent = .askChat
        default: return false
        }
        var head = 1
        if first == "show", words.count > 1, words[1].text == "me" { head = 2 }
        guard let pronoun = pronouns.filter({ pronoun in
            head + pronoun.count <= words.count && zip(pronoun, words[head...]).allSatisfy { $0 == $1.text }
        }).max(by: { $0.count < $1.count }) else { return false }
        var end = head + pronoun.count
        if intent == .askChat {
            while end < words.count, ["to", "that"].contains(words[end].text) { end += 1 }
            guard end < words.count else { return false }
            draft.task = String(draft.text[words[end].range.lowerBound...]).trimmedPrompt
        } else if end != words.count {
            // "Stop it from crashing" is not a command to stop it.
            return false
        }
        draft.intent = intent
        draft.sure = true
        draft.chat = chat.id
        return true
    }

    /// Where a phrase like "on every host" sits in `words`.
    static func everyComputer(in words: [VoiceWord]) -> Range<Int>? {
        if let index = words.firstIndex(where: { $0.text == "everywhere" }) { return index..<(index + 1) }
        for index in words.indices where everyWords.contains(words[index].text) {
            var noun = index + 1
            while noun < words.count, ["my", "the", "of", "our"].contains(words[noun].text) { noun += 1 }
            guard noun < words.count, computerNouns.contains(words[noun].text) else { continue }
            let start = index > 0 && ["on", "across", "to"].contains(words[index - 1].text) ? index - 1 : index
            return start..<(noun + 1)
        }
        return nil
    }
}

/// How long to wait for more words. A fixed pause is too short for someone
/// thinking and too long for "what needs me": what was said so far shows
/// which it is.
public enum VoiceEndpoint {
    /// Words a sentence does not end on.
    private static let dangling: Set<String> = [
        "a", "about", "an", "and", "asking", "at", "for", "from", "have", "in", "it", "my", "of", "on",
        "or", "tell", "telling", "that", "the", "then", "to", "using", "with",
    ]

    public static func pause(after draft: VoiceDraft, replying: Bool = false) -> Duration {
        if let last = VoiceNames.words(draft.text).last?.text, !replying, dangling.contains(last) { return .seconds(5) }
        if replying { return .seconds(1.5) }
        switch draft.intent {
        case .needsMe, .openChat, .stopChat, .answerChat: return .seconds(1.2)
        case .askChat, .newWorkspace: return draft.task == nil ? .seconds(3) : .seconds(2.5)
        case .none: return .seconds(3)
        }
    }
}

/// A short spoken answer to "say yes to start". Only a plain yes counts:
/// "yes but on the mini" is a correction, so nothing may start from it.
public enum VoiceReply: Equatable, Sendable {
    case yes, no, other

    private static let yeses: Set<String> = ["yes", "yeah", "yep", "yup", "go", "start", "start it", "go ahead", "do it", "confirm", "yes please", "ok", "okay"]
    private static let noes: Set<String> = ["no", "nope", "cancel", "stop", "never mind", "nevermind", "no thanks"]

    public init(_ text: String) {
        let said = VoiceNames.words(text).map(\.text).joined(separator: " ")
        self = Self.yeses.contains(said) ? .yes : Self.noes.contains(said) ? .no : .other
    }
}
