import Foundation

/// When this phone saw each chat of one computer answer, by pane, for
/// computers whose summaries carry no `lastTurnEndedMs`. A time is noted
/// only between two live homes, when the phone watched the change happen:
/// a change found after a relaunch or a return from the background
/// happened at some unknown moment while it was away.
public struct AnswerTimes: Codable, Hashable, Sendable {
    public private(set) var panes: [String: Date] = [:]

    public init() {}

    /// Follows `next` after `previous`, both live: a chat that stopped
    /// working or has a new answer answered `now`. Chats that left, or
    /// whose computer now dates them itself, are forgotten.
    public mutating func update(from previous: Home, to next: Home, now: Date) {
        let before = Dictionary(
            previous.workspaces.flatMap(\.chats).map { ($0.pane, $0) },
            uniquingKeysWith: { first, _ in first }
        )
        var kept: [String: Date] = [:]
        for chat in next.workspaces.flatMap(\.chats) {
            guard let summary = chat.summary, summary.lastTurnEndedMs == nil else { continue }
            if let old = before[chat.pane]?.summary,
               (old.working && !summary.working) || (summary.lastText != nil && summary.lastText != old.lastText) {
                kept[chat.pane] = now
            } else if let known = panes[chat.pane] {
                kept[chat.pane] = known
            }
        }
        panes = kept
    }

    public subscript(pane: String) -> Date? { panes[pane] }
}

extension Home.Summary {
    /// The agent is in a turn.
    var working: Bool { status == .working || turnStartedMs != nil }
}
