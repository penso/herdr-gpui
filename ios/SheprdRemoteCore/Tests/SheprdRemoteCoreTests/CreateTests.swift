import Foundation
import Testing
@testable import SheprdRemoteCore

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
        #expect(created.started == true)
        // A new chat's answer has no workspace, and a slow chat no pane.
        let chat = try JSONDecoder().decode(Created.self, from: Data(#"{"pane":null,"prompted":false}"#.utf8))
        #expect(chat.pane == nil && chat.workspace == nil && !chat.prompted)
        // A daemon too old for `start` says nothing, which is not a refusal.
        #expect(chat.started == nil)
    }

    @Test func requestsEncodeTheFieldsTheDaemonReads() throws {
        let body = try JSONEncoder().encode(NewWorkspaceRequest(cwd: "/a", branch: "fix", agent: "codex", prompt: "go"))
        let object = try #require(JSONSerialization.jsonObject(with: body) as? [String: String])
        #expect(object == ["cwd": "/a", "branch": "fix", "agent": "codex", "prompt": "go"])
    }

    /// The values `generated_branches_are_namespaced_and_reproducible`
    /// pins on the desktop, so the two clients keep proposing alike.
    @Test func proposedBranchesMatchTheDesktop() {
        #expect(NewWorkspaceRequest.proposedBranch(seed: 0) == "worktree/brave-river-0000")
        #expect(NewWorkspaceRequest.proposedBranch(seed: 9) == "worktree/calm-cloud-0009")
        #expect(NewWorkspaceRequest.proposedBranch(seed: .max) == "worktree/silver-valley-ffff")
    }

    @Test func clockProposalsAreSeededInMicroseconds() {
        let date = Date(timeIntervalSince1970: 1)
        #expect(NewWorkspaceRequest.proposedBranch(at: date) == NewWorkspaceRequest.proposedBranch(seed: 1_000_000))
        // A clock before 1970 still proposes a branch rather than trapping.
        #expect(NewWorkspaceRequest.proposedBranch(at: Date(timeIntervalSince1970: -1)) == "worktree/brave-river-0000")
    }
}
