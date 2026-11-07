import Foundation

/// Which chats hold words not sent yet, for the lists to mark. Drafts stay
/// on disk under hashed names, so each chat is asked about once, as it is
/// first listed; from then on this follows the composers, whose latest word
/// a save still on its way must not be overruled by.
///
/// iPad windows can show one chat, each with a composer of its own, so each
/// is followed apart: one that never held anything has no say over what
/// another holds.
public struct DraftMarks<Key: Hashable & Sendable>: Equatable, Sendable {
    private enum Composer: Equatable, Sendable {
        case untouched, holding, emptied
    }

    public private(set) var marked: Set<Key> = []
    private var known: Set<Key> = []
    private var composers: [Key: [UUID: Composer]] = [:]

    public init() {}

    /// A composer for `key` opened.
    public mutating func open(_ key: Key, composer: UUID) {
        composers[key, default: [:]][composer] = .untouched
    }

    /// A composer for `key` closed, after its last save: what it held is
    /// on disk now, so the mark stays.
    public mutating func close(_ key: Key, composer: UUID) {
        composers[key]?[composer] = nil
        if composers[key]?.isEmpty == true { composers[key] = nil }
    }

    /// What `composer` holds now, or what was saved with no composer open
    /// when it is `nil` or has closed, as a failed send coming back may be.
    /// Answers whether the saved draft should go: this emptied the chat's
    /// last draft.
    @discardableResult
    public mutating func set(_ key: Key, drafted: Bool, by composer: UUID? = nil) -> Bool {
        known.insert(key)
        if drafted {
            marked.insert(key)
            if let composer, composers[key]?[composer] != nil { composers[key]?[composer] = .holding }
            return false
        }
        if let composer, let state = composers[key]?[composer] {
            if state == .untouched { return false }
            composers[key]?[composer] = .emptied
        }
        guard composers[key]?.values.contains(.holding) != true else { return false }
        marked.remove(key)
        return true
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
        composers = composers.filter { !gone($0.key) }
    }
}
