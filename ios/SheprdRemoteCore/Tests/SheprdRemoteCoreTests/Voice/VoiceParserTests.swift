import Testing
@testable import SheprdRemoteCore

@Suite struct VoiceParserTests {
    private let catalog = VoiceSample.catalog()
    private func parse(_ text: String) -> VoiceDraft { VoiceParser.parse(text, catalog: catalog) }

    @Test func theWholeSentence() {
        let draft = parse("Create a worktree on studio about the login redirect bug and spin up Claude asking it to fix the redirect loop after OAuth")
        #expect(draft.intent == .newWorkspace)
        #expect(draft.sure)
        #expect(draft.computer == "studio")
        #expect(draft.agent == "claude")
        #expect(draft.topic == "login redirect bug")
        #expect(draft.worktree)
        #expect(draft.task == "fix the redirect loop after OAuth")
    }

    @Test func partialSentencesAlreadyFillWhatWasSaid() {
        let draft = parse("Create a worktree on studio about the login")
        #expect(draft.computer == "studio")
        #expect(draft.topic == "login")
        #expect(draft.agent == nil && draft.task == nil)
    }

    @Test func marksColorTheSpeakersOwnWords() {
        let draft = parse("Spin up Claude on studio to rewrite the hero")
        let marked = draft.marks.map { (String(draft.text[$0.range]), $0.slot) }
        #expect(marked.contains { $0 == "studio" && $1 == .computer })
        #expect(marked.contains { $0 == "Claude" && $1 == .agent })
        #expect(marked.contains { $0 == "rewrite the hero" && $1 == .task })
    }

    @Test func theTaskKeepsTheWordsSaid() {
        // A place in the task's middle is part of the task.
        #expect(parse("Tell Codex to move the settings on studio into a table, on the macbook").task
            == "move the settings on studio into a table")
    }

    @Test func aPlaceAfterTheTaskIsNotPartOfIt() {
        let draft = parse("Have Gemini look into the memory leak on studio")
        #expect(draft.computer == "studio")
        #expect(draft.task == "look into the memory leak")
    }

    @Test func aTaskCannotTurnIntoACommand() {
        // What is asked of the agent stays the agent's, whatever it says.
        let draft = parse("Have Claude stop the windows installer and delete everything")
        #expect(draft.intent == .newWorkspace)
        #expect(draft.task == "stop the windows installer and delete everything")
    }

    @Test func aCommonWordIsNotAProject() {
        #expect(parse("Make a new branch on studio for the release notes").project == nil)
        #expect(parse("Make a new branch in the release repo").project == "release")
    }

    @Test func theCloudIsNotClaude() {
        #expect(parse("On studio start Gemini and ask it to deploy to the cloud").agent == "gemini")
        #expect(parse("Worktree on studio about the cloud").agent == nil)
    }

    @Test func refusedVerbsDoNothing() {
        for text in ["Delete everything on studio", "Merge the pull request", "Wipe build-01"] {
            let draft = parse(text)
            #expect(draft.intent == .none)
            #expect(draft.sure)
        }
    }

    @Test func aReplyPicksTheProject() {
        var draft = parse("New worktree on studio")
        draft = VoiceParser.edit(draft, saying: "moltis", catalog: catalog)
        #expect(draft.project == "moltis")
    }

    @Test func aCorrectionChangesOnlyWhatItNames() {
        var draft = parse("Create a worktree on studio about caching with Claude asking it to add an etag")
        draft = VoiceParser.edit(draft, saying: "no, on the macbook", catalog: catalog)
        #expect(draft.computer == "mac")
        #expect(draft.agent == "claude")
        #expect(draft.task == "add an etag")
        draft = VoiceParser.edit(draft, saying: "use codex instead", catalog: catalog)
        #expect(draft.agent == "codex")
        draft = VoiceParser.edit(draft, saying: "call it etag support", catalog: catalog)
        #expect(draft.branch == "etag-support")
        draft = VoiceParser.edit(draft, saying: "no worktree", catalog: catalog)
        #expect(!draft.worktree)
    }

    @Test func aModelsInventionsAreDropped() {
        let draft = VoiceDraft(text: "open hero animation")
        let reading = VoiceReading(action: "open_chat", computer: "studio", project: "site", agent: "claude", chat: "Hero animation", task: "show the animation screen")
        let adopted = draft.adopting(reading, catalog: catalog)
        #expect(adopted.computer == nil)
        #expect(adopted.project == nil)
        #expect(adopted.agent == nil)
        #expect(adopted.task == nil)
        #expect(adopted.chat == "studio/p4")
        #expect(adopted.intent == .openChat)
    }

    @Test func aModelCannotActOnNothing() {
        // The real model read this as a message to a chat.
        let draft = VoiceDraft(text: "what's the weather like")
        #expect(draft.adopting(VoiceReading(action: "ask_chat", chat: "Hero animation"), catalog: catalog).intent == .none)
        #expect(draft.adopting(VoiceReading(action: "new_workspace"), catalog: catalog).intent == .none)
    }

    @Test func aModelCannotStopOrAnswer() {
        let draft = VoiceDraft(text: "what's the status of lighthouse pass")
        for action in ["stop_chat", "answer_chat"] {
            #expect(draft.adopting(VoiceReading(action: action, chat: "Lighthouse pass"), catalog: catalog).intent == .none)
        }
    }

    @Test func aModelsMessageNeedsTheSpeakersWords() {
        let draft = VoiceDraft(text: "lighthouse pass skip mobile")
        let invented = VoiceReading(action: "ask_chat", chat: "Lighthouse pass", task: "please skip the mobile audit")
        #expect(draft.adopting(invented, catalog: catalog).intent == .none)
        let quoted = VoiceReading(action: "ask_chat", chat: "Lighthouse pass", task: "skip mobile")
        #expect(draft.adopting(quoted, catalog: catalog).intent == .askChat)
    }

    @Test func addressedByName() {
        let draft = parse("Claude fix the failing test in sheprd on studio")
        #expect(draft.task == "fix the failing test")
        #expect(draft.project == "sheprd")
        #expect(draft.computer == "studio")
    }

    @Test func aTopicCanNameAComputer() {
        #expect(parse("Create a worktree about studio lighting").computer == nil)
    }

    @Test func aModelCannotOverruleTheRules() {
        var draft = parse("Delete everything on studio")
        draft = draft.adopting(VoiceReading(action: "new_workspace", computer: "studio"), catalog: catalog)
        #expect(draft.intent == .none)
    }
}
