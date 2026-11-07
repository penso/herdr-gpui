import Foundation

/// Reads a sentence with rules. It costs nothing, so it runs on every word
/// while the person speaks, it needs no model, and it is what a phone without
/// Apple Intelligence has. What it cannot place, `VoiceModel` tries.
public enum VoiceParser {
    private static let needsMe = [
        "needs me", "need me", "needs my attention", "need my attention", "waiting on me", "waiting for me",
        "what's waiting", "what is waiting", "anything waiting", "what's blocked", "what is blocked", "anything blocked",
    ].map { $0.split(separator: " ").map(String.init) }
    /// Sentences that open like this give a task, whatever it mentions.
    private static let commanding: Set<String> = [
        "ask", "create", "fire", "get", "have", "kick", "launch", "make", "new", "send", "spin", "start", "tell", "use", "worktree",
    ]
    private static let stopWords: Set<String> = ["stop", "cancel", "kill", "halt", "abort"]
    private static let openPhrases = ["open", "show me", "show", "go to", "jump to", "switch to", "take me to"]
    private static let allowWords: Set<String> = ["allow", "approve", "accept"]
    private static let denyWords: Set<String> = ["deny", "reject", "decline"]
    private static let askPhrases = ["send a message to", "message", "tell", "ask", "say to", "send"]
    private static let starters: Set<String> = [
        "create", "make", "new", "spin", "start", "launch", "kick", "fire", "worktree", "workspace", "branch", "need",
    ]
    private static let agentVerbs: Set<String> = ["have", "get", "ask", "tell", "use", "with", "spin", "start", "launch", "up", "and", "off"]
    private static let refused: Set<String> = [
        "archive", "close", "commit", "delete", "deploy", "destroy", "erase", "format", "merge", "push",
        "rebase", "remind", "remove", "reset", "revert", "turn", "wipe",
    ]
    /// How agents' names come back from the transcriber.
    private static let misheard = ["claude": ["cloud", "clawed", "claud", "clod"], "codex": ["code x", "codecs", "code decks"]]
    /// Agents a computer may not offer. Naming one still names it, so the
    /// plan asks rather than quietly starting another.
    private static let knownAgents = [
        AgentChoice(id: "claude", name: "Claude"),
        AgentChoice(id: "codex", name: "Codex"),
        AgentChoice(id: "copilot", name: "Copilot"),
        AgentChoice(id: "gemini", name: "Gemini"),
        AgentChoice(id: "opencode", name: "OpenCode"),
    ]
    private static let projectBefore: Set<String> = ["in", "inside", "into", "within", "under", "of", "from"]
    private static let projectAfter: Set<String> = ["repo", "repository", "project", "codebase", "checkout"]
    private static let skipped: Set<String> = ["the", "my", "our", "a"]
    private static let chatNouns: Set<String> = ["chat", "tab", "session", "agent"]

    /// Reads `text`, which may refer to `context`. In another language the
    /// rules can only pick out names; the model reads the rest.
    public static func parse(_ text: String, catalog: VoiceCatalog, context: VoiceContext? = nil, english: Bool = true) -> VoiceDraft {
        var draft = VoiceDraft(text: text)
        let words = VoiceNames.words(text)
        guard !words.isEmpty else { return draft }
        guard english else {
            draft = edit(draft, saying: text, catalog: catalog)
            draft.text = text
            return draft
        }
        if let repeated = repeating(context, saying: text, words: words, catalog: catalog) { return repeated }
        let spoken = words.filter { !fillers.contains($0.text) }
        if asksWhatNeedsMe(spoken) {
            draft.intent = .needsMe
            draft.sure = true
            return draft
        }
        if readPronoun(spoken, into: &draft, context: context) { return draft }
        if readChatCommand(spoken, into: &draft, catalog: catalog) { return draft }
        if let first = spoken.first?.text, refused.contains(first) {
            draft.sure = true
            return draft
        }
        readWorkspace(spoken, into: &draft, catalog: catalog)
        return draft
    }

    /// Whole words only: "needs memory" is not "needs me".
    private static func asksWhatNeedsMe(_ words: [VoiceWord]) -> Bool {
        guard let first = words.first?.text, !commanding.contains(first) else { return false }
        let said = words.map(\.text)
        return needsMe.contains { phrase in
            said.indices.contains { start in
                start + phrase.count <= said.count && Array(said[start..<(start + phrase.count)]) == phrase
            }
        }
    }

    private static let fillers: Set<String> = ["um", "uh", "er", "erm", "hmm", "okay", "ok", "so", "please", "hey"]

    // MARK: Chats

    /// A command for a chat that exists, which opens with its verb.
    private static func readChatCommand(_ words: [VoiceWord], into draft: inout VoiceDraft, catalog: VoiceCatalog) -> Bool {
        guard let first = words.first?.text else { return false }
        if stopWords.contains(first), let chat = chat(in: words, from: 1, catalog: catalog) {
            apply(chat, .stopChat, to: &draft, catalog: catalog)
            return true
        }
        if let length = phrase(openPhrases, at: words), let chat = chat(in: words, from: length, catalog: catalog) {
            apply(chat, .openChat, to: &draft, catalog: catalog)
            return true
        }
        if allowWords.contains(first) || denyWords.contains(first) {
            // Only a bare "allow" means the one chat waiting: naming a chat
            // that does not exist must not answer another.
            let bare = words.count == 1 || (words.count == 2 && ["it", "that", "this"].contains(words[1].text))
            let target = chat(in: words, from: 1, catalog: catalog)
                ?? (bare ? onlyWaiting(catalog).map { (chat: $0, end: words.count, range: nil) } : nil)
            guard let target else {
                draft.sure = true
                return true
            }
            draft.answer = allowWords.contains(first) ? .allow : .deny
            apply(target, .answerChat, to: &draft, catalog: catalog)
            return true
        }
        if first == "answer", let chat = chat(in: words, from: 1, catalog: catalog) {
            let rest = tail(of: words, from: chat.end, in: draft.text, skipping: ["with", "by", "saying", "that"])
            draft.answer = rest.map(VoiceAnswer.saying)
            apply(chat, .answerChat, to: &draft, catalog: catalog)
            return true
        }
        guard let length = phrase(askPhrases, at: words),
              let chat = chat(in: words, from: length, catalog: catalog, adjacent: true)
        else { return false }
        apply(chat, .askChat, to: &draft, catalog: catalog)
        draft.task = tail(of: words, from: chat.end, in: draft.text, skipping: ["to", "that", "saying", "and", "say"])
        if let range = draft.task.flatMap({ draft.text.range(of: $0) }) {
            draft.marks.append(VoiceMark(range: range, slot: .task))
        }
        return true
    }

    private typealias ChatSpot = (chat: VoiceCatalog.Chat, end: Int, range: Range<String.Index>?)

    private static func apply(_ spot: ChatSpot, _ intent: VoiceIntent, to draft: inout VoiceDraft, catalog: VoiceCatalog) {
        draft.intent = intent
        draft.sure = true
        draft.chat = spot.chat.id
        if let range = spot.range { draft.marks.append(VoiceMark(range: range, slot: .chat)) }
    }

    /// The chat named from word `start`. With `adjacent` it must come
    /// straight after: "tell Claude to …" is not about a chat that happens
    /// to mention something later in the sentence.
    private static func chat(in words: [VoiceWord], from start: Int, catalog: VoiceCatalog, adjacent: Bool = false) -> ChatSpot? {
        guard start < words.count else { return nil }
        var head = start
        while head < words.count, skipped.contains(words[head].text) { head += 1 }
        let rest = Array(words[head...])
        let found = catalog.chats.compactMap { chat -> (VoiceCatalog.Chat, VoiceSpot)? in
            VoiceNames.find(chat.title, in: rest, partly: true).map { (chat, $0) }
        }
        let near = adjacent ? found.filter { $0.1.words.lowerBound == 0 } : found
        guard let best = near.max(by: { $0.1.score < $1.1.score }) else { return nil }
        var end = head + best.1.words.upperBound
        if end < words.count, chatNouns.contains(words[end].text) { end += 1 }
        return (best.0, end, best.1.range)
    }

    private static func onlyWaiting(_ catalog: VoiceCatalog) -> VoiceCatalog.Chat? {
        let waiting = catalog.chats.filter { $0.question != nil }
        return waiting.count == 1 ? waiting[0] : nil
    }

    /// How many words of a phrase in `phrases` open `words`, the longest.
    private static func phrase(_ phrases: [String], at words: [VoiceWord]) -> Int? {
        phrases.compactMap { phrase -> Int? in
            let parts = phrase.split(separator: " ").map(String.init)
            guard parts.count <= words.count, zip(parts, words).allSatisfy({ $0 == $1.text }) else { return nil }
            return parts.count
        }.max()
    }

    /// The speaker's words from word `start` on, past any of `skipping`.
    private static func tail(of words: [VoiceWord], from start: Int, in text: String, skipping: Set<String>) -> String? {
        var head = start
        while head < words.count, skipping.contains(words[head].text) { head += 1 }
        guard head < words.count else { return nil }
        return String(text[words[head].range.lowerBound...]).trimmedPrompt
    }

    // MARK: Workspaces

    private static func readWorkspace(_ words: [VoiceWord], into draft: inout VoiceDraft, catalog: VoiceCatalog) {
        let agent = agentSpot(in: words, catalog: catalog)
        let marker = taskStart(in: words, agent: agent)
        let end = marker?.task
        let control = Array(words[..<(marker?.marker ?? words.count)])
        readControl(control, agent: agent, into: &draft, catalog: catalog)
        var taskFrom = end
        if taskFrom == nil { taskFrom = bareTask(in: words, draft: draft, agent: agent) }
        if let taskFrom, taskFrom < words.count {
            let taskEnd = peelPlaces(from: words[taskFrom...], into: &draft, catalog: catalog)
            if taskFrom < taskEnd {
                let range = words[taskFrom].range.lowerBound..<words[taskEnd - 1].range.upperBound
                draft.task = String(draft.text[range]).trimmedPrompt
                draft.marks.append(VoiceMark(range: range, slot: .task))
            }
        }
        if let id = agent?.id {
            draft.agent = id
            if let range = agent?.range { draft.marks.append(VoiceMark(range: range, slot: .agent)) }
        } else if let unknown = unknownAgent(in: control) {
            // Kept as said, so the plan asks instead of using another.
            draft.agent = unknown.text
            draft.marks.append(VoiceMark(range: unknown.range, slot: .agent))
        }
        let starts = words.first.map { starters.contains($0.text) } ?? false
        let shaped = words.contains { ["worktree", "workspace", "branch"].contains($0.text) }
        draft.worktree = words.contains { ["worktree", "branch"].contains($0.text) }
            || draft.topic != nil && shaped
        if starts || shaped || agent != nil {
            draft.intent = .newWorkspace
            draft.sure = true
        }
    }

    /// A sentence can say where after what: "fix the cache in moltis on
    /// studio". Takes such places off the end of the task and returns where
    /// the task ends, so the work does not run on the wrong computer with
    /// its place left in the prompt.
    private static func peelPlaces(from task: ArraySlice<VoiceWord>, into draft: inout VoiceDraft, catalog: VoiceCatalog) -> Int {
        var end = task.endIndex
        if let every = everyComputer(in: Array(task)), every.upperBound == task.count {
            draft.fanOut = .every
            end = task.startIndex + every.lowerBound
        }
        while end > task.startIndex {
            let rest = Array(task[task.startIndex..<end])
            if draft.computer == nil, let place = trailingPlace(in: rest, before: ["on", "at"], names: catalog.computers.map(\.name), partly: true) {
                draft.computer = catalog.computers.first { $0.name == place.name }?.id
                draft.marks.append(VoiceMark(range: place.range, slot: .computer))
                end = task.startIndex + place.start
            } else if draft.project == nil, let place = trailingPlace(in: rest, before: projectBefore, names: catalog.projectNames, partly: false) {
                draft.project = place.name
                draft.marks.append(VoiceMark(range: place.range, slot: .project))
                end = task.startIndex + place.start
            } else {
                break
            }
        }
        return end
    }

    /// A name at the very end of `words`, after one of `before` and any of
    /// "the" or "my", or with "repo" after it.
    private static func trailingPlace(in words: [VoiceWord], before: Set<String>, names: [String], partly: Bool) -> (name: String, range: Range<String.Index>, start: Int)? {
        var last = words.count
        if last > 0, projectAfter.contains(words[last - 1].text) { last -= 1 }
        let head = Array(words[..<last])
        let spots = names.compactMap { name -> (String, VoiceSpot)? in
            VoiceNames.find(name, in: head, partly: partly).map { (name, $0) }
        }.filter { $0.1.words.upperBound == head.count }
        guard let spot = spots.max(by: { $0.1.score < $1.1.score }) else { return nil }
        var word = spot.1.words.lowerBound - 1
        while word >= 0, skipped.contains(words[word].text) { word -= 1 }
        guard word >= 0, before.contains(words[word].text) else { return nil }
        return (spot.0, spot.1.range, word)
    }

    private typealias AgentSpot = (id: String, range: Range<String.Index>, words: Range<Int>)

    /// The agent to start. Its name must be plainly said, or come where an
    /// agent is expected: "the cloud" in a task is not Claude.
    private static func agentSpot(in words: [VoiceWord], catalog: VoiceCatalog) -> AgentSpot? {
        let known = knownAgents.filter { name in !catalog.agents.contains { $0.id == name.id } }
        for index in words.indices {
            for agent in catalog.agents + known {
                let names = [agent.id, agent.name.lowercased()] + (misheard[agent.id] ?? [])
                for name in names {
                    let parts = name.split(separator: " ").map(String.init)
                    guard index + parts.count <= words.count,
                          zip(parts, words[index...]).allSatisfy({ $0 == $1.text })
                    else { continue }
                    let plain = name == agent.id || name == agent.name.lowercased()
                    let expected = index == 0 || agentVerbs.contains(words[index - 1].text)
                    if plain || expected {
                        let last = words[index + parts.count - 1]
                        return (agent.id, words[index].range.lowerBound..<last.range.upperBound, index..<(index + parts.count))
                    }
                }
            }
        }
        return nil
    }

    /// Where the task starts when the sentence marks it: "asking it to",
    /// "tell it to", "have Claude", "get Codex to".
    /// `marker` is where those words start, and `task` where the task does.
    private static func taskStart(in words: [VoiceWord], agent: AgentSpot?) -> (marker: Int, task: Int)? {
        // Addressed by name: "Claude, fix the failing test".
        if let agent, agent.words.lowerBound == 0, agent.words.upperBound < words.count,
           !notTask.contains(words[agent.words.upperBound].text) {
            return (agent.words.upperBound, agent.words.upperBound)
        }
        let verbs: Set<String> = ["asking", "ask", "telling", "tell", "have", "having", "get", "getting", "make", "making"]
        let pronouns: Set<String> = ["it", "them", "him", "her"]
        for index in words.indices where verbs.contains(words[index].text) {
            guard index + 1 < words.count else { continue }
            let next = index + 1
            var after: Int?
            if pronouns.contains(words[next].text) {
                after = next + 1
            } else if let agent, agent.words.lowerBound == next {
                after = agent.words.upperBound
            }
            guard var after else { continue }
            while after < words.count, ["to", "like"].contains(words[after].text) { after += 1 }
            if after < words.count { return (index, after) }
        }
        return nil
    }

    /// Words that carry on after an agent's name rather than start a task.
    private static let notTask: Set<String> = [
        "on", "in", "at", "to", "and", "with", "for", "about", "using", "should", "will", "asking", "telling",
    ]

    /// A task with no marker: what follows the last thing recognized, past
    /// "to" or "and", as in "start Claude on studio to rewrite the hero".
    private static func bareTask(in words: [VoiceWord], draft: VoiceDraft, agent: AgentSpot?) -> Int? {
        guard agent != nil else { return nil }
        let marked = draft.marks.map(\.range.upperBound) + [agent?.range.upperBound].compactMap(\.self)
        guard let last = marked.max(),
              var index = words.firstIndex(where: { $0.range.lowerBound >= last })
        else { return nil }
        // Words that finish the phrase just recognized.
        while index < words.count, projectAfter.contains(words[index].text) || chatNouns.contains(words[index].text) { index += 1 }
        guard index < words.count else { return nil }
        let joiner = ["to", "and", "should", "will"].contains(words[index].text)
        let afterPause = words[index].range.lowerBound > last
            && draft.text[last..<words[index].range.lowerBound].contains(",")
        if joiner {
            index += 1
            while index < words.count, ["then", "to"].contains(words[index].text) { index += 1 }
            return index < words.count ? index : nil
        }
        return afterPause ? index : nil
    }

    /// Computer, project, and topic, from the words before any task.
    private static func readControl(_ words: [VoiceWord], agent: AgentSpot?, into draft: inout VoiceDraft, catalog: VoiceCatalog) {
        let lower = words.map(\.text).joined(separator: " ")
        if ["least busy", "most room", "most free", "has room", "idle machine"].contains(where: lower.contains) {
            draft.leastBusy = true
        }
        let computers = catalog.computers.compactMap { computer -> (VoiceCatalog.Computer, VoiceSpot)? in
            VoiceNames.find(computer.name, in: words, partly: true).map { (computer, $0) }
        }
        // "About studio lighting" says what it is about, not where.
        let named = computers.filter { !follows(topicWords, $0.1, in: words) }
        let computer = named.max { $0.1.score < $1.1.score }
        if computer == nil, !draft.leastBusy, everyComputer(in: words) == nil, let unknown = unknownComputer(in: words) {
            draft.unknownComputer = unknown.text
            draft.marks.append(VoiceMark(range: unknown.range, slot: .computer))
        }
        if named.count > 1 { draft.fanOut = .computers(named.sorted { $0.1.range.lowerBound < $1.1.range.lowerBound }.map(\.0.id)) }
        if everyComputer(in: words) != nil { draft.fanOut = .every }
        if let computer {
            draft.computer = computer.0.id
            draft.marks.append(VoiceMark(range: computer.1.range, slot: .computer))
        }
        let projects = catalog.projectNames.compactMap { name -> (String, VoiceSpot)? in
            guard let spot = VoiceNames.find(name, in: words), placed(spot, in: words),
                  computer.map({ !$0.1.range.overlaps(spot.range) }) ?? true
            else { return nil }
            return (name, spot)
        }
        if let project = projects.max(by: { $0.1.score < $1.1.score }) {
            draft.project = project.0
            draft.marks.append(VoiceMark(range: project.1.range, slot: .project))
        } else if let unknown = unknownProject(in: words) {
            // Kept as said, so the plan asks instead of using another.
            draft.project = unknown.text
            draft.marks.append(VoiceMark(range: unknown.range, slot: .project))
        }
        readTopic(words, agent: agent, into: &draft)
    }

    private static let phraseEnds: Set<String> = ["about", "with", "on", "in", "and", "for", "to", "using"]
    private static let notNames: Set<String> = [
        "new", "worktree", "branch", "workspace", "it", "this", "that", "parallel", "background", "them", "top",
    ]

    /// A single word after one of `before` that ends a phrase, which so has
    /// the shape of a name: "in nowhere about the login", "on zephyr".
    private static func nameShaped(after before: Set<String>, in words: [VoiceWord], ignoring: Set<String> = []) -> VoiceWord? {
        for index in words.indices where before.contains(words[index].text) {
            var name = index + 1
            while name < words.count, skipped.contains(words[name].text) { name += 1 }
            guard name < words.count, !notNames.contains(words[name].text), !ignoring.contains(words[name].text) else { continue }
            let next = name + 1
            if next == words.count || phraseEnds.contains(words[next].text) || projectAfter.contains(words[next].text) {
                return words[name]
            }
        }
        return nil
    }

    private static func unknownProject(in words: [VoiceWord]) -> VoiceWord? {
        nameShaped(after: projectBefore, in: words)
    }

    private static func unknownComputer(in words: [VoiceWord]) -> VoiceWord? {
        nameShaped(after: ["on", "at"], in: words)
    }

    /// "Start zorro on studio": no agent has that name, so none is chosen
    /// for it.
    private static func unknownAgent(in words: [VoiceWord]) -> VoiceWord? {
        let ordinary: Set<String> = ["working", "work", "up", "over", "fresh", "tests", "it", "chat", "session"]
        return nameShaped(after: ["start", "launch", "with", "using", "use"], in: words, ignoring: ordinary)
    }

    private static let topicWords: Set<String> = ["about", "for", "called", "named", "regarding"]

    private static func follows(_ openers: Set<String>, _ spot: VoiceSpot, in words: [VoiceWord]) -> Bool {
        var before = spot.words.lowerBound - 1
        while before >= 0, skipped.contains(words[before].text) { before -= 1 }
        return before >= 0 && openers.contains(words[before].text)
    }

    /// Whether a project's name comes where a project is expected. Project
    /// names are often ordinary words: "the release notes" names no project.
    private static func placed(_ spot: VoiceSpot, in words: [VoiceWord]) -> Bool {
        var before = spot.words.lowerBound - 1
        while before >= 0, skipped.contains(words[before].text) { before -= 1 }
        if before >= 0, projectBefore.contains(words[before].text) { return true }
        // The transcriber hears "in" as "and"; a plain project name settles it.
        if before >= 0, words[before].text == "and", spot.score >= 0.9 { return true }
        if spot.words.upperBound < words.count, projectAfter.contains(words[spot.words.upperBound].text) { return true }
        return false
    }

    /// What it is about: from "about", "for", or "called" to the next thing
    /// recognized.
    private static func readTopic(_ words: [VoiceWord], agent: AgentSpot?, into draft: inout VoiceDraft) {
        guard let opener = words.firstIndex(where: { topicWords.contains($0.text) }),
              opener + 1 < words.count
        else { return }
        var head = opener + 1
        while head < words.count, skipped.contains(words[head].text) { head += 1 }
        let stops: Set<String> = ["and", "with", "using", "use", "on", "in"]
        let taken = draft.marks.map(\.range) + [agent?.range].compactMap(\.self)
        var end = head
        while end < words.count, !stops.contains(words[end].text),
              !taken.contains(where: { $0.contains(words[end].range.lowerBound) }) {
            end += 1
        }
        guard end > head else { return }
        let range = words[head].range.lowerBound..<words[end - 1].range.upperBound
        draft.topic = String(draft.text[range])
        draft.marks.append(VoiceMark(range: range, slot: .topic))
    }

    // MARK: Changing a draft

    /// Applies something said about a draft already understood: an answer
    /// to its question, or a correction such as "no, on the mini". Names
    /// count wherever they are, as a short reply has no sentence around it.
    public static func edit(_ draft: VoiceDraft, saying text: String, catalog: VoiceCatalog) -> VoiceDraft {
        var edited = draft
        let words = VoiceNames.words(text)
        let lower = words.map(\.text).joined(separator: " ")
        if let agent = agentSpot(in: words, catalog: catalog) {
            edited.agent = agent.id
        } else if let unknown = unknownAgent(in: words) {
            edited.agent = unknown.text
        }
        if let spot = catalog.computers.compactMap({ computer in
            VoiceNames.find(computer.name, in: words, partly: true).map { (computer.id, $0.score) }
        }).max(by: { $0.1 < $1.1 }) {
            edited.computer = spot.0
            edited.unknownComputer = nil
            edited.leastBusy = false
        } else if let unknown = unknownComputer(in: words) {
            // The old computer is not what was asked for any more.
            edited.computer = nil
            edited.unknownComputer = unknown.text
        }
        if let project = catalog.projectNames.compactMap({ name in
            VoiceNames.find(name, in: words).map { (name, $0.score) }
        }).max(by: { $0.1 < $1.1 }) {
            edited.project = project.0
        } else if let unknown = unknownProject(in: words) {
            edited.project = unknown.text
        }
        if lower.contains("no worktree") || lower.contains("without a worktree") {
            edited.worktree = false
        } else if lower.contains("worktree") {
            edited.worktree = true
        }
        if let index = words.firstIndex(where: { ["call", "called", "named", "branch"].contains($0.text) }), index + 1 < words.count {
            let name = words[(index + 1)...].map(\.text).filter { !skipped.contains($0) && $0 != "it" }.joined(separator: "-")
            if !name.isEmpty {
                edited.branch = name
                edited.worktree = true
            }
        }
        if let task = taskStart(in: words, agent: nil)?.task {
            edited.task = String(text[words[task].range.lowerBound...]).trimmedPrompt
        }
        return edited
    }
}

extension String {
    /// A prompt as spoken, without hesitation sounds or trailing
    /// punctuation. Only sounds: "like" and "I guess" can be meant.
    var trimmedPrompt: String {
        var text = self
        for filler in [" um ", " uh ", " er ", " erm "] {
            text = text.replacingOccurrences(of: filler, with: " ", options: .caseInsensitive)
        }
        return text.trimmingCharacters(in: CharacterSet(charactersIn: " ,.;:\n"))
    }
}
