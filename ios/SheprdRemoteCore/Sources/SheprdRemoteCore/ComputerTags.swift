import Foundation

/// The labels a person gives their computers, such as `personal` or `work`.
/// Each computer's daemon keeps its own, and this phone a copy with each
/// `Computer` (`crates/sheprd-remote-control/src/control/tags.rs`). Two tags
/// differing only in case are one tag, spelled as it was first written.
public enum ComputerTags {
    public static let maxLength = 24
    public static let maxPerComputer = 8
    /// Offered while no computer has a tag yet.
    public static let starters = ["personal", "work"]

    /// `text` as a tag: single spaces, at most `maxLength` characters, or
    /// `nil` when nothing is left.
    public static func normalized(_ text: String) -> String? {
        let words = text.split(whereSeparator: \.isWhitespace).joined(separator: " ")
        let tag = String(words.prefix(maxLength)).trimmingCharacters(in: .whitespaces)
        return tag.isEmpty ? nil : tag
    }

    /// `tags` with `text` added, spelled as in `known` when it is already in
    /// use there. A repeat, an empty tag, or a full list changes nothing.
    public static func adding(_ text: String, to tags: [String], known: [String] = []) -> [String] {
        guard let tag = normalized(text), tags.count < maxPerComputer, !contains(tags, tag) else { return tags }
        return tags + [known.first { same($0, tag) } ?? tag]
    }

    /// Every tag in use, alphabetically.
    public static func all(in computers: [Computer]) -> [String] {
        unique(computers.flatMap(\.tags)).sorted { $0.localizedCaseInsensitiveCompare($1) == .orderedAscending }
    }

    /// The tags to offer `computer`: those its siblings use that it lacks,
    /// or the starters while none is in use.
    public static func suggestions(for computer: Computer, among computers: [Computer]) -> [String] {
        let known = all(in: computers)
        return (known.isEmpty ? starters : known).filter { !contains(computer.tags, $0) }
    }

    /// The tags `computer` keeps when it replaces records of `computers`, as
    /// pairing the same computer again does.
    public static func carried(to computer: Computer, from computers: [Computer]) -> [String] {
        let replaced = computers.filter { $0.fingerprint == computer.fingerprint || $0.device == computer.device }
        return Array(unique(computer.tags + replaced.flatMap(\.tags)).prefix(maxPerComputer))
    }

    /// What to do when the computer reports `remote`: the latest edit wins,
    /// so an edit made here while it was away is sent once it is back.
    public static func step(for computer: Computer, remote: Home.Tags) -> SyncStep {
        if remote.updatedMs > computer.tagsUpdatedMs { return .adopt }
        return remote.updatedMs < computer.tagsUpdatedMs ? .send : .none
    }

    /// The tag to filter by: `filter`, while a computer still has it. A
    /// filter whose last computer lost the tag would otherwise show nothing,
    /// with no pill left to clear it.
    public static func active(_ filter: String?, among computers: [Computer]) -> String? {
        guard let filter else { return nil }
        return all(in: computers).first { same($0, filter) }
    }

    static func contains(_ tags: [String], _ tag: String) -> Bool {
        tags.contains { same($0, tag) }
    }

    private static func same(_ one: String, _ other: String) -> Bool {
        one.caseInsensitiveCompare(other) == .orderedSame
    }

    private static func unique(_ tags: [String]) -> [String] {
        tags.reduce(into: []) { kept, tag in
            if !contains(kept, tag) { kept.append(tag) }
        }
    }
}

/// How a computer's tags and this phone's copy come together.
public enum SyncStep: Equatable, Sendable {
    /// They agree.
    case none
    /// The computer's are newer: this phone takes them.
    case adopt
    /// This phone's are newer: the computer is offered them.
    case send
}

/// The tag offers being sent, one at a time per computer. A new edit made
/// while another goes waits, and only the latest does: it holds every edit
/// before it. The edit going is not queued again, as a home that arrives
/// first would ask: the answer settles it, and a replay could undo what
/// another client did since.
public struct TagOffers<Offer: Sendable>: Sendable {
    private var going: [String: UInt64] = [:]
    private var next: [String: Offer] = [:]

    public init() {}

    /// Whether `offer`, the edit made at `edit`, can go now; if not, it
    /// waits for `finish`.
    public mutating func begin(_ device: String, _ offer: Offer, edit: UInt64) -> Bool {
        guard let sending = going[device] else {
            going[device] = edit
            return true
        }
        if sending != edit { next[device] = offer }
        return false
    }

    /// Ends the offer going to `device`, and returns the one to send next.
    public mutating func finish(_ device: String) -> Offer? {
        going[device] = nil
        return next.removeValue(forKey: device)
    }
}

extension Computer {
    /// Keeps the tags of the records in `computers` this one replaces, with
    /// when they were edited: an edit not yet sent must stay the latest.
    public mutating func carryTags(from computers: [Computer]) {
        let replaced = computers.filter { $0.fingerprint == fingerprint || $0.device == device }
        tags = ComputerTags.carried(to: self, from: computers)
        tagsUpdatedMs = replaced.map(\.tagsUpdatedMs).reduce(tagsUpdatedMs, max)
    }

    /// Takes the tags the computer says stand.
    public mutating func adopt(_ remote: Home.Tags) {
        tags = remote.tags
        tagsUpdatedMs = remote.updatedMs
    }

    /// Whether the computer shows under `filter`; no filter shows them all.
    public func matches(tag filter: String?) -> Bool {
        filter.map { ComputerTags.contains(tags, $0) } ?? true
    }
}
