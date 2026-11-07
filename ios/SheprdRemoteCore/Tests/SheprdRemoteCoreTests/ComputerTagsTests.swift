import Foundation
import SheprdRemoteCore
import Testing

@Suite struct ComputerTagsTests {
    private func computer(_ device: String, tags: [String] = [], fingerprint: String? = nil) throws -> Computer {
        let hex = fingerprint ?? String(repeating: "a", count: 63) + device.prefix(1)
        var computer = Computer(
            device: device, name: device, origins: [],
            fingerprint: try #require(Fingerprint(hex)), keyKind: .software, keyData: Data()
        )
        computer.tags = tags
        return computer
    }

    @Test func aTagIsTrimmedAndBounded() {
        #expect(ComputerTags.normalized("  side   project ") == "side project")
        #expect(ComputerTags.normalized(" \n ") == nil)
        let long = ComputerTags.normalized(String(repeating: "x", count: 100))
        #expect(long?.count == ComputerTags.maxLength)
    }

    @Test func addingIgnoresRepeatsWhateverTheirCase() {
        #expect(ComputerTags.adding("WORK", to: ["work"]) == ["work"])
        #expect(ComputerTags.adding("  ", to: ["work"]) == ["work"])
    }

    @Test func addingKeepsTheSpellingAlreadyInUse() {
        #expect(ComputerTags.adding("work", to: [], known: ["Work"]) == ["Work"])
    }

    @Test func aFullComputerTakesNoMoreTags() {
        let full = (0..<ComputerTags.maxPerComputer).map { "tag\($0)" }
        #expect(ComputerTags.adding("one more", to: full) == full)
    }

    @Test func everyTagInUseIsListedOnceInOrder() throws {
        let computers = [try computer("a", tags: ["work", "lab"]), try computer("b", tags: ["Work", "home"])]
        #expect(ComputerTags.all(in: computers) == ["home", "lab", "work"])
    }

    @Test func startersAreOfferedUntilATagIsInUse() throws {
        let bare = try computer("a")
        #expect(ComputerTags.suggestions(for: bare, among: [bare]) == ComputerTags.starters)
        let tagged = try computer("b", tags: ["lab"])
        #expect(ComputerTags.suggestions(for: bare, among: [bare, tagged]) == ["lab"])
        #expect(ComputerTags.suggestions(for: tagged, among: [bare, tagged]).isEmpty)
    }

    @Test func pairingAgainKeepsTheTags() throws {
        let hex = String(repeating: "b", count: 64)
        let old = try computer("old", tags: ["work"], fingerprint: hex)
        let other = try computer("c", tags: ["home"])
        let again = try computer("new", fingerprint: hex)
        #expect(ComputerTags.carried(to: again, from: [old, other]) == ["work"])
    }

    @Test func aFilterNoComputerHasIsDropped() throws {
        let computers = [try computer("a", tags: ["Work"])]
        #expect(ComputerTags.active("work", among: computers) == "Work")
        #expect(ComputerTags.active("home", among: computers) == nil)
        #expect(ComputerTags.active(nil, among: computers) == nil)
    }

    @Test func aFilterMatchesWhateverTheCase() throws {
        let tagged = try computer("a", tags: ["Work"])
        #expect(tagged.matches(tag: "work"))
        #expect(!tagged.matches(tag: "home"))
        #expect(tagged.matches(tag: nil))
    }

    @Test func aRecordSavedBeforeTagsHasNone() throws {
        let saved = try JSONEncoder().encode(try computer("a", tags: ["work"]))
        var record = try #require(JSONSerialization.jsonObject(with: saved) as? [String: Any])
        #expect(record["tags"] as? [String] == ["work"])
        record["tags"] = nil
        let old = try JSONSerialization.data(withJSONObject: record)
        #expect(try JSONDecoder().decode(Computer.self, from: old).tags.isEmpty)
    }

    @Test func theLatestEditDecidesWhichWayTagsGo() throws {
        var computer = try computer("a", tags: ["work"])
        computer.tagsUpdatedMs = 100
        #expect(ComputerTags.step(for: computer, remote: .init(tags: ["work"], updatedMs: 100)) == .none)
        #expect(ComputerTags.step(for: computer, remote: .init(tags: ["home"], updatedMs: 200)) == .adopt)
        #expect(ComputerTags.step(for: computer, remote: .init(tags: [], updatedMs: 0)) == .send)

        computer.adopt(.init(tags: ["home"], updatedMs: 200))
        #expect(computer.tags == ["home"])
        #expect(computer.tagsUpdatedMs == 200)
    }

    @Test func tagsFromBeforeSyncReachTheComputerOnceButLoseToAnyEdit() throws {
        let saved = try JSONEncoder().encode(try computer("a", tags: ["work"]))
        var record = try #require(JSONSerialization.jsonObject(with: saved) as? [String: Any])
        record["tagsUpdatedMs"] = nil
        let old = try JSONDecoder().decode(
            Computer.self, from: JSONSerialization.data(withJSONObject: record)
        )
        #expect(ComputerTags.step(for: old, remote: .init(tags: [], updatedMs: 0)) == .send)
        #expect(ComputerTags.step(for: old, remote: .init(tags: ["home"], updatedMs: 2)) == .adopt)

        record["tags"] = []
        let untagged = try JSONDecoder().decode(
            Computer.self, from: JSONSerialization.data(withJSONObject: record)
        )
        #expect(ComputerTags.step(for: untagged, remote: .init(tags: [], updatedMs: 0)) == .none)
    }

    @Test func aHomeFromAnOlderComputerHasNoTags() throws {
        let home = try JSONDecoder().decode(
            Home.self,
            from: Data(#"{"computer":"a","fingerprint":"b","connected":true,"waiting":0,"workspaces":[]}"#.utf8)
        )
        #expect(home.tags == nil)
    }

    @Test func aHomeCarriesTheTagsAndTheirTime() throws {
        let home = try JSONDecoder().decode(
            Home.self,
            from: Data(#"{"computer":"a","fingerprint":"b","connected":true,"waiting":0,"workspaces":[],"tags":{"tags":["work"],"updated_ms":7}}"#.utf8)
        )
        #expect(home.tags == Home.Tags(tags: ["work"], updatedMs: 7))
    }

    @Test func pairingAgainKeepsAnUnsentEditTheLatest() throws {
        var old = try computer("a", tags: ["work"])
        old.tagsUpdatedMs = 2_000
        var paired = try computer("a")
        paired.carryTags(from: [old, try computer("b", tags: ["lab"])])
        #expect(paired.tags == ["work"])
        #expect(paired.tagsUpdatedMs == 2_000)
        let older = Home.Tags(tags: ["personal"], updatedMs: 1_000)
        #expect(ComputerTags.step(for: paired, remote: older) == .send)
    }

    @Test func offersGoOneAtATimeAndOnlyTheLatestWaits() {
        var offers = TagOffers<Int>()
        let first = offers.begin("a", 1, edit: 10)
        let other = offers.begin("b", 1, edit: 10)
        let second = offers.begin("a", 2, edit: 20)
        let third = offers.begin("a", 3, edit: 30)
        #expect(first && other && !second && !third)
        let next = offers.finish("a")
        let none = offers.finish("a")
        let otherNext = offers.finish("b")
        #expect(next == 3 && none == nil && otherNext == nil)
        let again = offers.begin("a", 4, edit: 40)
        #expect(again)
    }

    @Test func theEditGoingIsNotQueuedAgain() {
        var offers = TagOffers<Int>()
        let first = offers.begin("a", 1, edit: 10)
        let replay = offers.begin("a", 1, edit: 10)
        #expect(first && !replay)
        let next = offers.finish("a")
        #expect(next == nil)
    }
}
