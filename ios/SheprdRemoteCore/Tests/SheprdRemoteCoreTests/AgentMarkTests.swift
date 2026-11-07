import Foundation
import Testing
@testable import SheprdRemoteCore

/// The logo a row draws for the agent Herdr names.
struct AgentMarkTests {
    @Test func anAgentShowsItsOwnLogo() {
        #expect(AgentMark.named("claude") == "claude")
        #expect(AgentMark.named("Claude") == "claude")
        #expect(AgentMark.named("grok") == "grok")
        #expect(AgentMark.named("cursor") == "cursor")
        // "copilot" holds "pi"; it keeps its own logo.
        #expect(AgentMark.named("copilot") == "copilot")
        #expect(AgentMark.named("pi") == "pi")
        #expect(AgentMark.named("kimi") == "generic")
        #expect(AgentMark.named(nil) == "generic")
    }

    /// Each logo is in the app's asset catalog, the same file Sheprd and
    /// the web app draw.
    @Test func everyLogoIsTheOneSheprdDraws() throws {
        let ios = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent()
        for mark in AgentMark.all {
            let shipped = ios.appending(path: "SheprdRemote/Assets.xcassets/Agents/\(mark).imageset/\(mark).svg")
            let drawn = ios.deletingLastPathComponent().appending(path: "assets/icons/agent-\(mark).svg")
            #expect(try Data(contentsOf: shipped) == Data(contentsOf: drawn), "\(mark)")
        }
    }
}
