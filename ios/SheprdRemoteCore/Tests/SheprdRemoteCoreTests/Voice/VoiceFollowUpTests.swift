import Testing
@testable import SheprdRemoteCore

@Suite struct VoiceFollowUpTests {
    private let catalog = VoiceSample.catalog()

    private var started: VoiceContext {
        let draft = VoiceParser.parse("Worktree on studio in moltis about caching with Claude asking it to add an etag", catalog: catalog)
        return VoiceContext(draft: draft, chat: catalog.chats[3])
    }

    private func parse(_ text: String) -> VoiceDraft { VoiceParser.parse(text, catalog: catalog, context: started) }

    @Test func theSameThingElsewhereKeepsTheRest() {
        let draft = parse("Same thing on the macbook")
        #expect(draft.intent == .newWorkspace)
        #expect(draft.computer == "mac")
        #expect(draft.agent == "claude")
        #expect(draft.task == "add an etag")
        #expect(draft.text == "Same thing on the macbook")
    }

    @Test func theSameWithAnotherAgent() {
        #expect(parse("Do the same with Codex").agent == "codex")
    }

    @Test(arguments: ["Stop it", "Stop that one", "Cancel the last one"])
    func aPronounMeansTheLastChat(text: String) {
        let draft = parse(text)
        #expect(draft.intent == .stopChat)
        #expect(draft.chat == "studio/p4")
    }

    @Test func openItAndTellIt() {
        #expect(parse("Open it").intent == .openChat)
        #expect(parse("Show me that one").intent == .openChat)
        let told = parse("Tell it to use the staging database")
        #expect(told.intent == .askChat)
        #expect(told.task == "use the staging database")
    }

    @Test func withNothingBeforeAPronounMeansNothing() {
        #expect(VoiceParser.parse("Stop it", catalog: catalog).intent == .none)
        #expect(VoiceParser.parse("Same thing on the macbook", catalog: catalog).agent == nil)
    }

    @Test func aPronounInsideASentenceIsNotAReference() {
        #expect(parse("Stop it from crashing on studio").chat == nil)
    }

    @Test func everyComputer() {
        let draft = VoiceParser.parse("Start Claude in sheprd on every host asking it to run the tests", catalog: catalog)
        #expect(draft.fanOut == .every)
        guard case let .fanOut(plan) = VoicePlan.resolve(draft, catalog: catalog) else {
            Issue.record("expected a fan-out")
            return
        }
        #expect(plan.plans.compactMap(\.computer?.id) == ["mac", "studio", "build"])
        #expect(plan.plans.allSatisfy { $0.prompt == "run the tests" })
        #expect(plan.skipped.isEmpty)
    }

    @Test func everyComputerAfterTheTask() {
        let draft = VoiceParser.parse("Have Claude run the tests in sheprd on all my machines", catalog: catalog)
        #expect(draft.fanOut == .every)
        #expect(draft.task == "run the tests")
        #expect(draft.project == "sheprd")
    }

    @Test func aComputerWithoutTheProjectIsSkippedAndSaid() {
        let draft = VoiceParser.parse("Start Claude in site on every computer asking it to build", catalog: catalog)
        guard case let .fanOut(plan) = VoicePlan.resolve(draft, catalog: catalog) else {
            Issue.record("expected a fan-out")
            return
        }
        #expect(plan.plans.compactMap(\.computer?.id) == ["studio"])
        #expect(plan.skipped == ["MacBook Pro", "build-01"])
    }

    @Test func twoNamedComputers() {
        let draft = VoiceParser.parse("Start Claude in sheprd on studio and build-01 asking it to build", catalog: catalog)
        #expect(draft.fanOut == .computers(["studio", "build"]))
    }

    @Test func anOfflineComputerIsSkipped() {
        let catalog = VoiceSample.catalog(studioOnline: false)
        let draft = VoiceParser.parse("Start Claude in sheprd on every host", catalog: catalog)
        guard case let .fanOut(plan) = VoicePlan.resolve(draft, catalog: catalog) else {
            Issue.record("expected a fan-out")
            return
        }
        #expect(plan.skipped == ["studio"])
    }

    @Test func aTerminalAgentTakesWordsButNoAnswers() {
        var catalog = VoiceSample.catalog()
        catalog.chats.append(.init(device: "studio", pane: "t1", title: "Nightly migration", computer: "studio", terminal: true))
        guard case let .chat(asked) = VoicePlan.resolve(VoiceParser.parse("Tell nightly migration to carry on", catalog: catalog), catalog: catalog) else {
            Issue.record("expected a chat")
            return
        }
        #expect(asked.chat.terminal)
        #expect(asked.action == .ask("carry on"))
        guard case let .chat(answered) = VoicePlan.resolve(VoiceParser.parse("Allow nightly migration", catalog: catalog), catalog: catalog) else {
            Issue.record("expected a chat")
            return
        }
        #expect(answered.action == .unanswerable)
    }
}

@Suite struct VoiceEndpointTests {
    private let catalog = VoiceSample.catalog()
    private func pause(_ text: String) -> Duration { VoiceEndpoint.pause(after: VoiceParser.parse(text, catalog: catalog)) }

    @Test func anUnfinishedSentenceWaitsLongest() {
        #expect(pause("Create a worktree on studio and") == .seconds(5))
        #expect(pause("Start Claude asking it to") == .seconds(5))
    }

    @Test func aFinishedShortCommandEndsSoon() {
        #expect(pause("What needs me") == .seconds(1.2))
        #expect(pause("Stop hero animation") == .seconds(1.2))
    }

    @Test func aTaskCanStillGrow() {
        #expect(pause("Start Claude asking it to fix the login") == .seconds(2.5))
        #expect(pause("Create a worktree on studio about login") == .seconds(3))
    }

    @Test func aReplyIsShort() {
        #expect(VoiceEndpoint.pause(after: VoiceDraft(text: "moltis"), replying: true) == .seconds(1.5))
    }
}

@Suite struct VoiceConfirmTests {
    @Test(arguments: ["Yes", "yes.", "Yeah", "go ahead", "Do it", "OK"])
    func aPlainYes(text: String) { #expect(VoiceReply(text) == .yes) }

    @Test(arguments: ["No", "cancel", "Never mind"])
    func aPlainNo(text: String) { #expect(VoiceReply(text) == .no) }

    @Test(arguments: ["Yes but on the macbook", "yes on studio", "not yes", "go to hero animation", ""])
    func anythingElseStartsNothing(text: String) { #expect(VoiceReply(text) == .other) }

    private func plan(_ text: String) -> VoicePlan {
        let catalog = VoiceSample.catalog()
        return VoicePlan.resolve(VoiceParser.parse(text, catalog: catalog), catalog: catalog)
    }

    @Test func onlyANewWorktreeTakesASpokenYes() {
        #expect(plan("Worktree on studio about login with Claude asking it to fix it").confirmableByVoice)
        // Not one working in a checkout that already exists.
        #expect(!plan("Start Claude on studio in site to rewrite the hero").confirmableByVoice)
        #expect(!plan("Stop hero animation").confirmableByVoice)
        #expect(!plan("Tell hero animation to use video").confirmableByVoice)
        #expect(!plan("Start Claude in sheprd on every host").confirmableByVoice)
        #expect(!plan("Worktree in nowhere about login with Claude").confirmableByVoice)
    }

    @Test func theReadBackSaysWhatWillRun() {
        #expect(plan("Worktree on studio in moltis about login with Claude asking it to fix it").readBack
            == "Claude on studio, in moltis to fix it. Say yes to start.")
    }
}
