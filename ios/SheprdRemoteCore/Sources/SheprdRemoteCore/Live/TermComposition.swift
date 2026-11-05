import Foundation

/// An input method's composition (Pinyin, Kana, Hangul) in the terminal's
/// keyboard: marked text stays on the phone until a candidate is picked,
/// and only the committed text is typed into the pane.
public struct TermComposition: Equatable, Sendable {
    /// What the input method is composing, not yet typed.
    public private(set) var marked = ""

    public init() {}

    /// The input method's new marked text; nothing is typed yet.
    public mutating func mark(_ text: String) {
        marked = text
    }

    /// Text to type: a picked candidate replaces the marked text.
    public mutating func insert(_ text: String) -> String? {
        marked = ""
        return text.isEmpty ? nil : text
    }

    /// The marked text, committed as it stands.
    public mutating func unmark() -> String? {
        defer { marked = "" }
        return marked.isEmpty ? nil : marked
    }

    /// Whether the pane should get a backspace: while composing, it
    /// deletes from the marked text instead.
    public mutating func deleteBackward() -> Bool {
        guard !marked.isEmpty else { return true }
        marked.removeLast()
        return false
    }
}
