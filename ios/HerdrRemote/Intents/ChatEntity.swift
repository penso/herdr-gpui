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
    /// Spotlight was emptied for the app lock since it was last turned on;
    /// set only once the removal succeeded, so a failed one is tried again.
    private var clearedForLock = false
    /// The last change sent to Spotlight. Each waits for the one before,
    /// so an index still in flight cannot land after a removal.
    private var last: Task<Void, Never>?

    /// The app lock hides chat names from Spotlight too. What an earlier
    /// launch indexed is not in `indexed`, so turning the lock on, or
    /// launching with it on, empties the index of every chat.
    func update(_ summary: WidgetSummary) {
        guard !AppLock.isOn else {
            indexed = [:]
            guard !clearedForLock else { return }
            enqueue { [weak self] in
                let removed = (try? await CSSearchableIndex.default().deleteAppEntities(ofType: ChatEntity.self)) != nil
                // Unlocked meanwhile: the next lock must remove again.
                self?.clearedForLock = removed && AppLock.isOn
            }
            return
        }
        clearedForLock = false
        let chats = summary.chats
        let current = Dictionary(chats.map { ($0.id, "\($0.title)|\($0.computer)|\($0.workspace)") }, uniquingKeysWith: { first, _ in first })
        guard current != indexed else { return }
        let gone = Set(indexed.keys).subtracting(current.keys)
        let changed = chats.filter { indexed[$0.id] != current[$0.id] }.map { ChatEntity($0) }
        indexed = current
        enqueue {
            let index = CSSearchableIndex.default()
            if !gone.isEmpty { try? await index.deleteAppEntities(identifiedBy: Array(gone), ofType: ChatEntity.self) }
            if !changed.isEmpty { try? await index.indexAppEntities(changed) }
            // Locked while this ran: what it indexed goes again.
            if AppLock.isOn { try? await index.deleteAppEntities(ofType: ChatEntity.self) }
        }
    }

    private func enqueue(_ work: @escaping @MainActor () async -> Void) {
        let previous = last
        last = Task {
            await previous?.value
            await work()
        }
    }
}
