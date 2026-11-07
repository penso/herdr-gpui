import Foundation
import Testing
@testable import SheprdRemoteCore

/// A fleet to speak to: three computers and the chats on them.
enum VoiceSample {
    static let agents = [
        AgentChoice(id: "claude", name: "Claude"),
        AgentChoice(id: "codex", name: "Codex"),
        AgentChoice(id: "gemini", name: "Gemini"),
    ]

    static func options(_ folders: [String]) -> NewOptions {
        NewOptions(agents: agents, recent: folders, roots: [])
    }

    static func catalog(studioOnline: Bool = true) -> VoiceCatalog {
        VoiceCatalog(
            computers: [
                .init(id: "mac", name: "MacBook Pro", online: true, options: options(["/Users/me/code/sheprd", "/Users/me/code/moltis"])),
                .init(id: "studio", name: "studio", online: studioOnline, options: options(["/Users/me/code/site", "/Users/me/code/sheprd", "/Users/me/code/moltis"])),
                .init(id: "build", name: "build-01", online: true, options: options(["/srv/release", "/srv/sheprd"])),
            ],
            chats: [
                .init(device: "mac", pane: "p1", title: "Equalize splits command", computer: "MacBook Pro"),
                .init(device: "mac", pane: "p2", title: "Fix mise discovery on SSH", computer: "MacBook Pro", question: question),
                .init(device: "mac", pane: "p3", title: "Voice pipeline latency", computer: "MacBook Pro"),
                .init(device: "studio", pane: "p4", title: "Hero animation", computer: "studio"),
                .init(device: "studio", pane: "p5", title: "Lighthouse pass", computer: "studio"),
                .init(device: "build", pane: "p6", title: "Windows installer", computer: "build-01"),
            ],
            recentDevice: "mac",
            recentAgent: "codex"
        )
    }

    static let question: Permission = {
        let json = #"""
        {"id":1,"title":"Run git push?","options":[
          {"id":"a","name":"Allow once","kind":"allow_once"},
          {"id":"b","name":"Always allow","kind":"allow_always"},
          {"id":"c","name":"Deny","kind":"reject_once"}]}
        """#
        return try! JSONDecoder().decode(Permission.self, from: Data(json.utf8))
    }()
}

/// One sentence and what it should come to.
struct VoiceCase: Decodable {
    var u: String
    var action: String
    var computer: String?
    var project: String?
    var agent: String?
    var chat: String?
    var task: String?

    static func load(_ name: String) throws -> [VoiceCase] {
        let url = try #require(Bundle.module.url(forResource: "Fixtures/voice/\(name)", withExtension: "json"))
        return try JSONDecoder().decode([VoiceCase].self, from: Data(contentsOf: url))
    }

    /// What `draft` got wrong, by field. A field the case leaves out is
    /// only checked where a wrong value would act on the wrong thing.
    func misses(_ draft: VoiceDraft, catalog: VoiceCatalog) -> [String] {
        var misses: [String] = []
        if draft.intent.rawValue != action { misses.append("action=\(draft.intent.rawValue)") }
        guard action != "none", action != "needs_me" else { return misses }
        let computerName = catalog.computers.first { $0.id == draft.computer }?.name
        if action == "new_workspace" {
            if computerName != computer { misses.append("computer=\(computerName ?? "nil")") }
            if let project, draft.project != project { misses.append("project=\(draft.project ?? "nil")") }
            if let agent, draft.agent != agent { misses.append("agent=\(draft.agent ?? "nil")") }
        } else {
            let title = catalog.chats.first { $0.id == draft.chat }?.title
            if title != chat { misses.append("chat=\(title ?? "nil")") }
        }
        if let task, !VoiceCase.same(task, draft.task ?? "") { misses.append("task=\(draft.task ?? "nil")") }
        return misses
    }

    private static func same(_ a: String, _ b: String) -> Bool {
        VoiceNames.words(a).map(\.text) == VoiceNames.words(b).map(\.text)
    }
}
