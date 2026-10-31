import Foundation
import Testing
@testable import HerdrRemoteCore

/// The new workspace and new chat answers, from the daemon's own types.
@Suite struct CreateTests {
    @Test func newOptionsListAgentsAndOfferedFolders() throws {
        let options = try JSONDecoder().decode(NewOptions.self, from: WireTests.fixture("new"))
        #expect(options.agents.first == AgentChoice(id: "claude", name: "Claude"))
        #expect(options.roots.first?.path == "/Users/me/code")
        // Recent first, and a folder both lists hold appears once.
        #expect(options.folders == ["/Users/me/code/herdr-gpui", "/Users/me/code/moltis"])
    }

    @Test func createdCarriesThePaneAndWhetherThePromptWent() throws {
        let created = try JSONDecoder().decode(Created.self, from: WireTests.fixture("created"))
        #expect(created.pane == "p7")
        #expect(created.workspace == "w3")
        #expect(created.prompted)
        // A new chat's answer has no workspace, and a slow chat no pane.
        let chat = try JSONDecoder().decode(Created.self, from: Data(#"{"pane":null,"prompted":false}"#.utf8))
        #expect(chat.pane == nil && chat.workspace == nil && !chat.prompted)
    }

    @Test func requestsEncodeTheFieldsTheDaemonReads() throws {
        let body = try JSONEncoder().encode(NewWorkspaceRequest(cwd: "/a", branch: "fix", agent: "codex", prompt: "go"))
        let object = try #require(JSONSerialization.jsonObject(with: body) as? [String: String])
        #expect(object == ["cwd": "/a", "branch": "fix", "agent": "codex", "prompt": "go"])
    }
}
