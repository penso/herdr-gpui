import Foundation
import HerdrRemoteCore

/// What widgets, Controls, Siri, Spotlight, and the share extension know of
/// the computers without a connection: the chats needing you and running,
/// per computer, as the app last saw them. A small file in the App Group,
/// readable after the first unlock so lock screen widgets can draw it; the
/// full cache stays readable only while unlocked.
struct WidgetSummary: Codable, Hashable, Sendable {
    struct Chat: Codable, Hashable, Sendable, Identifiable {
        var device: String
        /// The computer's CA fingerprint, which links and commands use.
        var computerId: String
        var computer: String
        var pane: String
        var title: String
        var workspace: String
        var workspaceId: String
        var agent: String?
        var needsYou: Bool
        var working: Bool
        /// The question waiting, with its answers, for the Watch.
        var question: Question?

        var id: String { "\(device)|\(pane)" }

        var link: URL {
            ChatLink(computer: computer, computerId: computerId, pane: pane).url
        }
    }

    /// A waiting permission request, as the Watch answers it.
    struct Question: Codable, Hashable, Sendable {
        var permission: PermissionId
        var title: String
        var options: [ChatActivityState.Option]
    }

    var chats: [Chat] = []
    var updated = Date.distantPast

    var waiting: Int { chats.filter(\.needsYou).count }
    var working: Int { chats.filter(\.working).count }

    /// Chats needing you first, then running ones, then the rest.
    var ranked: [Chat] {
        chats.filter(\.needsYou) + chats.filter { $0.working && !$0.needsYou } + chats.filter { !$0.working && !$0.needsYou }
    }

    static let kind = "so.pen.herdr.remote.attention"

    #if os(iOS)
    private static var file: URL? {
        FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: "group.so.pen.herdr.remote")?
            .appending(path: "summary.json")
    }

    static func load() -> WidgetSummary {
        guard let file, let data = try? Data(contentsOf: file),
              let summary = try? JSONDecoder().decode(WidgetSummary.self, from: data)
        else { return WidgetSummary() }
        return summary
    }

    /// Writes it; answers whether it changed, so callers reload widgets
    /// only then.
    @discardableResult
    func save() -> Bool {
        guard let file = Self.file else { return false }
        var previous = Self.load()
        previous.updated = updated
        guard previous != self, let data = try? JSONEncoder().encode(self) else { return false }
        return (try? data.write(to: file, options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication])) != nil
    }
    #endif
}
