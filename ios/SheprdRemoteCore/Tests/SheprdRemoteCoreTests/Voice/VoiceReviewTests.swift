import Testing
@testable import SheprdRemoteCore

/// What the first review of voice commands found.
@Suite struct VoiceReviewTests {
    private let catalog = VoiceSample.catalog()
    private func parse(_ text: String) -> VoiceDraft { VoiceParser.parse(text, catalog: catalog) }

    private func question(_ draft: VoiceDraft) -> VoicePlan.Question? {
        if case let .workspace(plan) = VoicePlan.resolve(draft, catalog: catalog) { plan.question } else { nil }
    }

    @Test func aNamedComputerThatDoesNotExistIsAsked() {
        let draft = parse("Start Claude on zephyr")
        #expect(draft.computer == nil)
        #expect(question(draft) == .unknownComputer("zephyr"))
        guard case let .workspace(plan) = VoicePlan.resolve(draft, catalog: catalog) else {
            Issue.record("expected a workspace")
            return
        }
        #expect(plan.computer == nil)
        #expect(plan.request(start: nil) == nil)
    }

    @Test func pickingAComputerSettlesIt() {
        var draft = VoiceParser.edit(parse("Start Claude on zephyr"), saying: "on studio", catalog: catalog)
        #expect(question(draft) == nil)
        #expect(draft.computer == "studio")
        draft.unknownComputer = "zephyr"
        #expect(question(draft) == nil)
    }

    @Test func aTaskCanSayOnWithoutNamingAComputer() {
        #expect(parse("Start Claude asking it to work on login").unknownComputer == nil)
        #expect(parse("Start Claude on the least busy machine").unknownComputer == nil)
        #expect(parse("Start Claude on every host").unknownComputer == nil)
    }

    @Test func aCorrectionToAnUnknownComputerDropsTheOldOne() {
        let draft = VoiceParser.edit(parse("Start Claude on studio"), saying: "no, on zephyr", catalog: catalog)
        #expect(draft.computer == nil)
        #expect(question(draft) == .unknownComputer("zephyr"))
    }

    @Test func aCorrectionToAnUnknownProjectDropsTheOldOne() {
        let draft = VoiceParser.edit(parse("Start Claude on studio in moltis"), saying: "no, in nowhere", catalog: catalog)
        #expect(draft.project == "nowhere")
        guard case .project = question(draft) else {
            Issue.record("expected a project question")
            return
        }
    }

    @Test(arguments: ["Start zorro on studio", "New worktree on studio with zorro", "Worktree on studio using zorro"])
    func aNamedAgentThatDoesNotExistIsAsked(text: String) {
        #expect(question(parse(text)) == .agent(on: "studio", named: "zorro"))
    }

    @Test func aCorrectionToAnUnknownAgentIsAsked() {
        let draft = VoiceParser.edit(parse("Start Claude on studio"), saying: "use zorro", catalog: catalog)
        #expect(question(draft) == .agent(on: "studio", named: "zorro"))
    }

    @Test func allowingAChatThatDoesNotExistAnswersNothing() {
        // One question waits, on another chat.
        let draft = parse("Allow nonexistent chat")
        #expect(draft.intent == .none)
        #expect(draft.chat == nil)
        #expect(draft.sure)
    }

    @Test(arguments: ["Allow", "Allow it", "Deny that"])
    func aBareAnswerMeansTheOneChatWaiting(text: String) {
        let draft = parse(text)
        #expect(draft.intent == .answerChat)
        #expect(draft.chat == "mac/p2")
    }

    @Test func aBareAnswerWithSeveralWaitingAnswersNothing() {
        var catalog = VoiceSample.catalog()
        catalog.chats[3].question = VoiceSample.question
        #expect(VoiceParser.parse("Allow", catalog: catalog).intent == .none)
    }

    @Test func aTaskIsNotAStatusQuestion() {
        let draft = parse("Ask Claude to explain why this service needs memory")
        #expect(draft.intent == .newWorkspace)
        #expect(draft.task == "explain why this service needs memory")
        #expect(parse("Have Claude check what is blocked on the deploy").intent == .newWorkspace)
        #expect(parse("Something needs memory").intent != .needsMe)
    }

    @Test(arguments: ["What needs me?", "what’s waiting", "Is anything waiting on me right now"])
    func statusQuestionsStillWork(text: String) {
        #expect(parse(text).intent == .needsMe)
    }

    @Test func aPromptKeepsItsWords() {
        #expect(parse("Get Claude to make the query like the old one").task == "make the query like the old one")
        #expect(parse("Have Claude say I guess in the tooltip").task == "say I guess in the tooltip")
        #expect(parse("Have Claude um fix uh the login").task == "fix the login")
    }

    @Test func givenValuesAreLookedUpOnTheirOwnLists() {
        // Siri's project and agent, kept apart: an agent's name is not a
        // project, even on a computer with a project called that.
        let computer = VoiceCatalog.Computer(id: "a", name: "a", online: true, options: VoiceSample.options(["/code/claude", "/code/site"]))
        #expect(computer.project(named: "nowhere") == nil)
        #expect(computer.project(named: "Claude nowhere") == nil)
        #expect(computer.project(named: "site")?.folder == "/code/site")
        #expect(computer.agent(named: "Claude")?.id == "claude")
        #expect(computer.agent(named: "site") == nil)
    }

    @Test func anUnattendedChatNeverBypassesPermissions() {
        let bypass = OfferedMode(id: "bypassPermissions", name: "Bypass Permissions")
        let plan = OfferedMode(id: "plan", name: "Plan")
        // A mode that runs without asking is dropped, whatever else is saved.
        #expect(StartChoice(autoAccept: true, mode: bypass.id, offered: [bypass, plan]).unattendedStart == nil)
        let picked = StartChoice(autoAccept: true, mode: plan.id, offered: [bypass, plan]).unattendedStart
        #expect(picked == ChatStart(mode: "plan", autoAccept: false))
        #expect(StartChoice(autoAccept: true).unattendedStart == nil)
    }
}
