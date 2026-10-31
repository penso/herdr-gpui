import Foundation

/// What was just typed into a terminal, shown above the keys so typing is
/// never blind. Each input keeps one entry, in the order sent. The line
/// stays whole while typing goes on, rather than losing each character as
/// the screen shows it, which flickers on a fast network; it clears once
/// the computer took everything and typing paused for `quiet`.
public struct TermEcho: Sendable {
    public typealias Instant = ContinuousClock.Instant

    /// How long typing must pause before a fully taken line clears.
    public static let quiet: Duration = .seconds(2)

    private enum State: Sendable, Equatable {
        case queued
        case sent
        case taken(Instant)
    }

    private struct Entry: Sendable {
        /// The key's character, `⏎` or `⇥`, `nil` for backspace, and empty
        /// for what shows nothing (arrows, chords, pastes).
        var glyph: String?
        var state: State
    }

    private var entries: [Entry] = []
    /// When the last input was typed.
    private var typedAt: Instant?

    public init() {}

    /// What the strip shows: the typed characters, with backspaces taking
    /// back the ones before them, and `⌫` for one that reaches further.
    public var text: String {
        var shown: [Character] = []
        for entry in entries {
            guard let glyph = entry.glyph else {
                if let last = shown.last, last != "⌫" { shown.removeLast() } else { shown.append("⌫") }
                continue
            }
            shown.append(contentsOf: glyph)
        }
        return String(shown)
    }

    /// Whether the computer has taken every input on the line.
    public var allTaken: Bool {
        entries.allSatisfy { if case .taken = $0.state { true } else { false } }
    }

    /// Input waiting for its request, one entry per key or paste, typed
    /// at `now`.
    public mutating func queue(_ send: TermInput.Send, at now: Instant) {
        typedAt = now
        switch send {
        case let .keys(keys): entries += keys.map { Entry(glyph: Self.glyph(for: $0), state: .queued) }
        case .paste: entries.append(Entry(glyph: "", state: .queued))
        }
    }

    /// The next `count` queued inputs left in one request.
    public mutating func sending(_ count: Int, at now: Instant) {
        var left = count
        for index in entries.indices where left > 0 && entries[index].state == .queued {
            entries[index].state = .sent
            left -= 1
        }
    }

    /// The computer took the request out.
    public mutating func delivered(at now: Instant) {
        for index in entries.indices where entries[index].state == .sent {
            entries[index].state = .taken(now)
        }
    }

    /// Clears the line when the computer took all of it and nothing was
    /// typed for `quiet` before `now`.
    public mutating func settle(at now: Instant) {
        guard allTaken, let typedAt, now - typedAt >= Self.quiet else { return }
        entries.removeAll()
    }

    /// A request failed: what followed it is dropped too.
    public mutating func clear() {
        entries.removeAll()
    }

    /// What a key in Herdr's syntax shows.
    static func glyph(for key: String) -> String? {
        if key.count == 1 { return key }
        switch key {
        case "space": return " "
        case "plus": return "+"
        case "minus": return "-"
        case "slash": return "/"
        case "backtick": return "`"
        case "enter": return "⏎"
        case "tab": return "⇥"
        case "backspace": return nil
        default: return ""
        }
    }
}
