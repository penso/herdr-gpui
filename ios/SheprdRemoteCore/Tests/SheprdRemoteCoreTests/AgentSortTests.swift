import Foundation
import Testing
@testable import SheprdRemoteCore

/// Home lists agents by the order the person chose, the newest first.
struct AgentSortTests {
    private struct Row {
        var name: String
        var status: Home.AgentStatus = .idle
        var age: TimeInterval?
        var group = "g"
    }

    private static func sections(_ sort: AgentSort, _ rows: [Row]) -> [[String]] {
        sort.sections(of: rows) { row in
            AgentSort.Facts(status: row.status, date: row.age.map { Date(timeIntervalSince1970: 1_000_000 - $0) },
                            title: row.name, group: row.group, groupTitle: "title \(row.group)")
        }
        .map { $0.items.map(\.name) }
    }

    /// The screenshot's bug: a Friday answer above one from 13 hours ago.
    @Test func theNewestComesFirstWhateverOrderTheyArrivedIn() {
        let rows = [Row(name: "friday", age: 400_000), Row(name: "hours", age: 46_000), Row(name: "monday", age: 100_000)]
        #expect(Self.sections(.recent, rows) == [["hours", "monday", "friday"]])
    }

    @Test func undatedRowsFollowByTitle() {
        let rows = [Row(name: "b"), Row(name: "dated", age: 900_000), Row(name: "a")]
        #expect(Self.sections(.recent, rows) == [["dated", "a", "b"]])
    }

    @Test func priorityFollowsTheDesktopsStatusOrder() {
        let rows = [
            Row(name: "idle", status: .idle, age: 1), Row(name: "unknown", status: .unknown, age: 2),
            Row(name: "working", status: .working), Row(name: "done", status: .done), Row(name: "asking", status: .blocked),
        ]
        #expect(Self.sections(.priority, rows) == [["asking"], ["done"], ["working"], ["idle", "unknown"]])
    }

    @Test func priorityTitlesItsSectionsAndSkipsEmptyOnes() {
        let sections = AgentSort.priority.sections(of: [Home.AgentStatus.done, .blocked]) {
            AgentSort.Facts(status: $0, date: nil, title: "", group: "", groupTitle: "")
        }
        #expect(sections.map(\.title) == ["Needs you", "Done"])
    }

    @Test func workspaceKeepsTheirOrderWithTheNewestFirstInEach() {
        let rows = [
            Row(name: "old", age: 500, group: "second"), Row(name: "other", age: 1, group: "first"),
            Row(name: "new", age: 5, group: "second"),
        ]
        #expect(Self.sections(.workspace, rows) == [["new", "old"], ["other"]])
    }

    @Test func workspacesSharingANameStayApart() {
        let sections = AgentSort.workspace.sections(of: ["a", "b"]) {
            AgentSort.Facts(status: .idle, date: nil, title: $0, group: $0, groupTitle: "herdr-gpui · studio")
        }
        #expect(sections.map(\.title) == ["herdr-gpui · studio", "herdr-gpui · studio"])
        #expect(Set(sections.map(\.id)).count == 2)
    }

    @Test func nothingToListHasNoSections() {
        for sort in AgentSort.allCases { #expect(Self.sections(sort, []).isEmpty) }
    }
}
