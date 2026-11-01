import Foundation
import Testing
@testable import HerdrRemoteCore

/// An agent run in a terminal tab lists on Home and opens its own pane.
struct TerminalAgentTests {
    @Test func decodesTheAgentsPaneAndTitle() throws {
        guard case let .home(home) = try WireTests.event("home") else { Issue.record("not home"); return }
        let terminal = try #require(home.workspaces.first?.terminals.first)
        #expect(terminal.agent?.pane == "p2")
        #expect(terminal.agent?.title == "Fix the build")
        #expect(terminal.agentPane?.pane == "p2")
    }

    @Test func opensTheAgentsPaneRatherThanTheFocusedOne() {
        var terminal = row(agentPane: "p2")
        #expect(terminal.agentPane?.pane == "p2")
        // An older computer names no pane; a pane gone since is not opened.
        terminal.agent?.pane = nil
        #expect(terminal.agentPane?.pane == "p1")
        terminal.agent?.pane = "p9"
        #expect(terminal.agentPane?.pane == "p1")
    }

    @Test func decodesAnAgentFromAnOlderComputer() throws {
        let json = Data(#"{"name":"claude","status":"working"}"#.utf8)
        let agent = try JSONDecoder().decode(Home.TerminalRow.DetectedAgent.self, from: json)
        #expect(agent.status == .working)
        #expect(agent.pane == nil)
        #expect(agent.title == nil)
    }

    private func row(agentPane: String) -> Home.TerminalRow {
        Home.TerminalRow(
            tab: "t1",
            label: "zsh",
            cwd: nil,
            zoomed: false,
            panes: [
                .init(pane: "p1", label: nil, cwd: nil, focused: true),
                .init(pane: "p2", label: nil, cwd: nil, focused: false),
            ],
            agent: .init(name: "claude", status: .idle, pane: agentPane, title: nil)
        )
    }
}
