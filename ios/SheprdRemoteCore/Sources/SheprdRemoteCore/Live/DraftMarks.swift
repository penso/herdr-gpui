/// Which chats hold words not sent yet, for the lists to mark. Drafts stay
/// on disk under hashed names, so each chat is asked about once, as it is
/// first listed; from then on this follows the composer, whose latest word
/// a save still on its way must not be overruled by.
public struct DraftMarks<Key: Hashable & Sendable>: Equatable, Sendable {
    public private(set) var marked: Set<Key> = []
    private var known: Set<Key> = []

    public init() {}

    /// What the composer holds for `key` now.
    public mutating func set(_ key: Key, drafted: Bool) {
        known.insert(key)
        if drafted { marked.insert(key) } else { marked.remove(key) }
    }

    /// Asks `saved` about the `keys` not known yet.
    public mutating func discover(_ keys: some Sequence<Key>, saved: (Key) -> Bool) {
        for key in keys where known.insert(key).inserted {
            if saved(key) { marked.insert(key) }
        }
    }

    /// Drops every key `gone` matches, as forgetting a computer does.
    public mutating func forget(where gone: (Key) -> Bool) {
        marked = marked.filter { !gone($0) }
        known = known.filter { !gone($0) }
    }
}
