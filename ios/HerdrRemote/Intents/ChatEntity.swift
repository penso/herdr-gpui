import AppIntents
import CoreSpotlight
import HerdrRemoteCore

/// A chat on a paired computer, for Siri, Shortcuts, and Spotlight, as
/// the app last saw it (`WidgetSummary`).
struct ChatEntity: AppEntity, IndexedEntity {
    static let typeDisplayRepresentation: TypeDisplayRepresentation = "Herdr Chat"
    static let defaultQuery = ChatQuery()

    /// `device|pane`, as `WidgetSummary.Chat.id`.
    let id: String
    private(set) var title: String
    private(set) var computer: String
    let computerId: String
    private(set) var workspace: String
    let device: String
    let pane: String
    let needsYou: Bool

    init(_ chat: WidgetSummary.Chat) {
        id = chat.id
        title = chat.title
        computer = chat.computer
        computerId = chat.computerId
        workspace = chat.workspace
        device = chat.device
        pane = chat.pane
        needsYou = chat.needsYou
    }

    /// `chat` without its title, computer, or workspace, for while the
    /// app lock hides them: a saved shortcut still finds and opens it.
    init(hiding chat: WidgetSummary.Chat) {
        self.init(chat)
        title = "Chat"
        computer = ""
        workspace = ""
    }

    var displayRepresentation: DisplayRepresentation {
        DisplayRepresentation(
            title: "\(title)",
            subtitle: "\(computer) · \(workspace)",
            image: .init(systemName: needsYou ? "hand.raised.fill" : "bubble.left.and.text.bubble.right")
        )
    }

    var route: Route { .chat(PaneKey(device: device, pane: pane), title: title) }
}

/// While the app lock is on (Settings > Privacy), Siri, Shortcuts, and
/// Spotlight get no chat names: nothing is suggested or searched, and a
/// chat named by id comes back without its title.
struct ChatQuery: EntityStringQuery {
    func entities(for identifiers: [ChatEntity.ID]) async throws -> [ChatEntity] {
        let wanted = Set(identifiers)
        let chats = WidgetSummary.load().chats.filter { wanted.contains($0.id) }
        return AppLock.isOn ? chats.map { ChatEntity(hiding: $0) } : chats.map { ChatEntity($0) }
    }

    func entities(matching string: String) async throws -> [ChatEntity] {
        guard !AppLock.isOn else { return [] }
        return WidgetSummary.load().ranked
            .filter { $0.title.localizedCaseInsensitiveContains(string) || $0.workspace.localizedCaseInsensitiveContains(string) }
            .map { ChatEntity($0) }
    }

    func suggestedEntities() async throws -> [ChatEntity] {
        guard !AppLock.isOn else { return [] }
        return WidgetSummary.load().ranked.prefix(12).map { ChatEntity($0) }
    }
}

/// Keeps Spotlight's chats in step with the summary: new and renamed ones
/// indexed, gone ones removed.
@MainActor
final class SpotlightChats {
    private var indexed: [String: String] = [:]

    func update(_ summary: WidgetSummary) {
        // The app lock hides chat names from Spotlight too.
        let chats = AppLock.isOn ? [] : summary.chats
        let current = Dictionary(chats.map { ($0.id, "\($0.title)|\($0.computer)|\($0.workspace)") }, uniquingKeysWith: { first, _ in first })
        guard current != indexed else { return }
        let gone = Set(indexed.keys).subtracting(current.keys)
        let changed = chats.filter { indexed[$0.id] != current[$0.id] }.map { ChatEntity($0) }
        indexed = current
        Task {
            let index = CSSearchableIndex.default()
            if !gone.isEmpty { try? await index.deleteAppEntities(identifiedBy: Array(gone), ofType: ChatEntity.self) }
            if !changed.isEmpty { try? await index.indexAppEntities(changed) }
        }
    }
}
