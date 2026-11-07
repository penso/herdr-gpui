import Testing
@testable import SheprdRemoteCore

@Suite struct VoicePlanTests {
    private func plan(_ text: String, _ catalog: VoiceCatalog = VoiceSample.catalog()) -> VoicePlan {
        VoicePlan.resolve(VoiceParser.parse(text, catalog: catalog), catalog: catalog)
    }

    private func workspace(_ text: String, _ catalog: VoiceCatalog = VoiceSample.catalog()) throws -> VoicePlan.Workspace {
        guard case let .workspace(plan) = plan(text, catalog) else { throw Failure.notAWorkspace }
        return plan
    }

    private enum Failure: Error { case notAWorkspace }

    @Test func whatWasNotSaidIsFilledInAndLabeled() throws {
        let plan = try workspace("New worktree on studio about login bug asking it to fix it")
        #expect(plan.computer?.id == "studio")
        #expect(plan.computerSource == .said)
        #expect(plan.project?.name == "site")
        #expect(plan.projectSource == .recent)
        #expect(plan.agent?.id == "codex")
        #expect(plan.agentSource == .recent)
        #expect(plan.branch == "fix/login")
    }

    @Test func theRequestIsWhatTheCardShows() throws {
        let plan = try workspace("Worktree on studio in moltis about caching with Claude asking it to add an etag")
        let request = try #require(plan.request(start: nil))
        #expect(request.cwd == "/Users/me/code/moltis")
        #expect(request.branch == "feature/caching")
        #expect(request.agent == "claude")
        #expect(request.prompt == "add an etag")
    }

    @Test func noWorktreeUnlessAskedFor() throws {
        #expect(try workspace("Spin up Claude on studio in site to rewrite the hero").branch == "")
    }

    @Test func aProjectNotOnTheNamedComputerIsAsked() throws {
        let plan = try workspace("Start Claude on build-01 in moltis")
        guard case let .project(on, named, choices) = plan.question else {
            Issue.record("expected a project question")
            return
        }
        #expect(on == "build-01")
        #expect(named == "moltis")
        #expect(choices.map(\.name) == ["release", "sheprd"])
        #expect(plan.request(start: nil) == nil)
    }

    @Test func aProjectFindsAComputerThatHasIt() throws {
        let plan = try workspace("New worktree in site about the footer")
        #expect(plan.computer?.id == "studio")
    }

    @Test func anOfflineComputerIsNeverSwapped() throws {
        let plan = try workspace("Start Claude on studio", VoiceSample.catalog(studioOnline: false))
        #expect(plan.question == .offline("studio"))
        #expect(plan.request(start: nil) == nil)
    }

    @Test func nothingOnlineAsksForAComputer() throws {
        var catalog = VoiceSample.catalog()
        for index in catalog.computers.indices { catalog.computers[index].online = false }
        #expect(try workspace("Start Claude", catalog).question == .computer)
    }

    @Test func anAgentNotInstalledThereIsAsked() throws {
        var catalog = VoiceSample.catalog()
        catalog.computers[1].options?.agents = [AgentChoice(id: "codex", name: "Codex")]
        let plan = try workspace("Start Claude on studio", catalog)
        #expect(plan.question == .agent(on: "studio", named: "claude"))
        #expect(plan.agent == nil)
    }

    /// Seen in the simulator: with Claude not installed anywhere, "with
    /// Claude" showed Codex, the last one used.
    @Test func aNamedAgentNoComputerOffersIsNeverSwapped() throws {
        var catalog = VoiceSample.catalog()
        for index in catalog.computers.indices {
            catalog.computers[index].options?.agents = [AgentChoice(id: "codex", name: "Codex")]
        }
        let plan = try workspace("New worktree about login with Claude", catalog)
        #expect(plan.agent == nil)
        #expect(plan.question == .agent(on: "MacBook Pro", named: "claude"))
    }

    @Test func aNamedProjectThatDoesNotExistIsNeverSwapped() throws {
        let plan = try workspace("New worktree in nowhere about login with Claude")
        guard case let .project(_, named, _) = plan.question else {
            Issue.record("expected a project question")
            return
        }
        #expect(named == "nowhere")
        #expect(plan.project == nil)
    }

    @Test func aPhraseIsNotAnUnknownProject() throws {
        #expect(try workspace("Start Claude in a new worktree about login").question == nil)
    }

    @Test func onlyShowingSkipsTheConfirmation() {
        #expect(!plan("What needs me").needsConfirmation)
        #expect(!plan("Open hero animation").needsConfirmation)
        #expect(plan("Stop hero animation").needsConfirmation)
        #expect(plan("Tell hero animation to use video").needsConfirmation)
        #expect(plan("Start Claude on studio").needsConfirmation)
    }

    @Test func allowingAnswersOnceNeverAlways() {
        guard case let .chat(chat) = plan("Allow fix mise discovery"), case let .answer(option, _) = chat.action else {
            Issue.record("expected an answer")
            return
        }
        #expect(option.kind == .allowOnce)
        guard case let .chat(denied) = plan("Deny fix mise discovery"), case let .answer(no, _) = denied.action else {
            Issue.record("expected an answer")
            return
        }
        #expect(no.kind == .rejectOnce)
    }

    @Test func aSpokenAnswerCannotChooseAlways() {
        guard case let .chat(chat) = plan("Answer fix mise discovery with always allow") else {
            Issue.record("expected a chat")
            return
        }
        if case let .answer(option, _) = chat.action { #expect(option.kind != .allowAlways) }
    }

    @Test func aChatWithNoQuestionCannotBeAnswered() {
        guard case let .chat(chat) = plan("Allow hero animation") else {
            Issue.record("expected a chat")
            return
        }
        #expect(chat.action == .unanswerable)
    }

    @Test func aHandOffPutsTheWholeSentenceInThePrompt() {
        let catalog = VoiceSample.catalog()
        let draft = VoiceParser.parse("Find the repo where we fixed auth", catalog: catalog)
        guard case let .workspace(plan) = VoicePlan.handOff(draft, catalog: catalog) else {
            Issue.record("expected a workspace")
            return
        }
        #expect(plan.prompt == "Find the repo where we fixed auth")
        #expect(plan.computer?.id == "mac")
        #expect(plan.branch.isEmpty)
    }

    @Test(arguments: [
        ("the login redirect bug", "fix/login-redirect"),
        ("dark mode", "feature/dark-mode"),
        ("Café menu!", "feature/caf-menu"),
    ])
    func branches(topic: String, branch: String) {
        #expect(VoiceBranch.name(for: topic) == branch)
    }

    @Test func onlyATopicAboutTheSameWorkIsDoubted() {
        #expect(VoiceBranch.doubtful(topic: "login, redirect fog", task: "fix the redirect loop"))
        #expect(!VoiceBranch.doubtful(topic: "caching", task: "add an etag"))
        #expect(!VoiceBranch.doubtful(topic: "the redirect loop", task: "fix the redirect loop"))
    }

    @Test func aTidiedBranchKeepsItsKind() {
        #expect(VoiceBranch.name(for: "redirect loop", topic: "login redirect bug") == "fix/redirect-loop")
        #expect(VoiceBranch.name(for: "redirect loop", topic: "login redirect") == "feature/redirect-loop")
    }

    @Test func aBranchStaysShortAndValid() throws {
        let name = try #require(VoiceBranch.name(for: String(repeating: "wordy phrasing ", count: 20)))
        #expect(name.count < 50)
        #expect(!name.contains(" ") && !name.hasSuffix("-"))
        #expect(VoiceBranch.name(for: "the") == nil)
    }
}
