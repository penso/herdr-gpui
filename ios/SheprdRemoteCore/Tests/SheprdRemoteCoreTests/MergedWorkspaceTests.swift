import Foundation
import Testing
@testable import SheprdRemoteCore

struct MergedWorkspaceTests {
    private func workspace(tone: String?) throws -> Home.Workspace {
        let pullRequest = tone.map { #","pull_request":{"number":7,"url":"https://example.com/7","tone":"\#($0)"}"# } ?? ""
        let json = #"{"id":"w","label":"w","cwd":"/code","chats":[],"terminals":[]\#(pullRequest)}"#
        return try JSONDecoder().decode(Home.Workspace.self, from: Data(json.utf8))
    }

    @Test func onlyAMergedPullRequestMarksTheWorkspaceDone() throws {
        #expect(try workspace(tone: "merged").merged)
        #expect(try !workspace(tone: "ready").merged)
        #expect(try !workspace(tone: nil).merged)
    }
}
