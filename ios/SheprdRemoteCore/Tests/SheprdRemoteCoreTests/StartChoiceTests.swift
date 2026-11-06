import Foundation
import Testing
@testable import SheprdRemoteCore

/// What new chats start with, and what the daemon is asked for.
@Suite struct StartChoiceTests {
    private let modes = [
        OfferedMode(id: "default", name: "Default"),
        OfferedMode(id: "bypassPermissions", name: "Bypass Permissions"),
    ]

    @Test func nothingPickedAsksNothing() {
        #expect(StartChoice(offered: modes).start == nil)
    }

    @Test func aPickedModeAndAutoAcceptAreAskedFor() {
        var choice = StartChoice(offered: modes)
        choice.follow(.setMode("bypassPermissions"))
        choice.follow(.setAutoAccept(true))
        #expect(choice.start == ChatStart(mode: "bypassPermissions", autoAccept: true))
        #expect(choice.picked?.name == "Bypass Permissions")
    }

    @Test func aModeNoLongerOfferedIsNotAskedFor() {
        var choice = StartChoice(offered: modes)
        choice.follow(.setMode("gone"))
        #expect(choice.picked == nil && choice.start == nil)
    }

    @Test func aModeSettingIsFollowedAndNamedInTheRequest() {
        var choice = StartChoice(offered: [OfferedMode(id: "plan", name: "Plan", option: "mode")])
        choice.follow(.setOption(id: "model", value: .choice("plan")))
        #expect(choice.mode == nil)
        choice.follow(.setOption(id: "mode", value: .choice("plan")))
        #expect(choice.start == ChatStart(mode: "plan", option: "mode"))
    }

    @Test func otherCommandsChangeNothing() {
        var choice = StartChoice(offered: modes)
        choice.follow(.cancel)
        choice.follow(.prompt(text: "hi"))
        #expect(choice == StartChoice(offered: modes))
    }

    @Test func runningWithoutAskingIsSaidForEitherReason() {
        #expect(!StartChoice(mode: "default", offered: modes).bypasses)
        #expect(StartChoice(mode: "bypassPermissions", offered: modes).bypasses)
        #expect(StartChoice(autoAccept: true).bypasses)
    }

    @Test func codexFullAccessCountsAsRunningWithoutAsking() {
        #expect(OfferedMode(id: "full-access", name: "Full Access").bypasses)
        #expect(!OfferedMode(id: "auto", name: "Auto").bypasses)
        #expect(!OfferedMode(id: "read-only", name: "Read Only").bypasses)
    }

    @Test func theLabelSaysWhatIsNotTheAgentsDefault() {
        #expect(StartChoice(offered: modes).label == nil)
        #expect(StartChoice(autoAccept: true, offered: modes).label == "Auto-accept")
        #expect(StartChoice(mode: "default", offered: modes).label == "Default")
        #expect(StartChoice(autoAccept: true, mode: "bypassPermissions", offered: modes).label == "Bypass Permissions · Auto-accept")
    }

    @Test func onlyModesAndAutoAcceptAreSettingsToOrder() {
        let choice = StartChoice(offered: [OfferedMode(id: "plan", name: "Plan", option: "mode")])
        #expect(choice.setting(of: .setMode("plan")) == .mode)
        #expect(choice.setting(of: .setOption(id: "mode", value: .choice("plan"))) == .mode)
        #expect(choice.setting(of: .setOption(id: "effort", value: .choice("high"))) == nil)
        #expect(choice.setting(of: .setAutoAccept(true)) == .autoAccept)
        #expect(choice.setting(of: .cancel) == nil)
    }

    @Test func anOlderPickFinishingLastIsNotTheLatest() {
        var picks = StartPicks()
        let on = picks.begin(agent: "Claude", setting: .autoAccept)
        let off = picks.begin(agent: "Claude", setting: .autoAccept)
        #expect(!picks.isLatest(on, agent: "Claude", setting: .autoAccept))
        #expect(picks.isLatest(off, agent: "Claude", setting: .autoAccept))
    }

    @Test func picksOfOtherSettingsAndAgentsAreIndependent() {
        var picks = StartPicks()
        let mode = picks.begin(agent: "Claude", setting: .mode)
        _ = picks.begin(agent: "Claude", setting: .autoAccept)
        _ = picks.begin(agent: "Codex", setting: .mode)
        #expect(picks.isLatest(mode, agent: "Claude", setting: .mode))
    }

    @Test func aChoiceSurvivesBeingSaved() throws {
        let choice = StartChoice(autoAccept: true, mode: "default", offered: modes)
        let saved = try JSONEncoder().encode(choice)
        #expect(try JSONDecoder().decode(StartChoice.self, from: saved) == choice)
    }

    @Test func theRequestCarriesTheFieldsTheDaemonReads() throws {
        let request = NewChatRequest(agent: "claude", start: ChatStart(mode: "plan", option: "mode", autoAccept: true))
        let body = try JSONEncoder().encode(request)
        let object = try #require(JSONSerialization.jsonObject(with: body) as? [String: Any])
        let start = try #require(object["start"] as? [String: Any])
        #expect(start["mode"] as? String == "plan")
        #expect(start["option"] as? String == "mode")
        #expect(start["auto_accept"] as? Bool == true)
    }

    @Test func aDaemonThatDoesNotSayStartedDidNot() throws {
        let created = try JSONDecoder().decode(Created.self, from: Data(#"{"pane":"p1","prompted":false}"#.utf8))
        #expect(!created.started)
    }
}
