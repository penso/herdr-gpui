import Foundation

/// An input method's composition (Pinyin, Kana, Hangul) in the terminal's
/// keyboard: marked text stays on the phone until a candidate is picked,
/// and only the committed text is typed into the pane.
public struct TermComposition: Equatable, Sendable {
    /// What the input method is composing, not yet typed.
    public private(set) var marked = ""
    /// The input method's selection inside `marked`, in UTF-16 as UIKit
    /// counts; a caret when empty.
    public private(set) var selection = NSRange()

    public init() {}

    /// The input method's new marked text, with its caret at `selected`,
    /// or at the end; nothing is typed yet.
    public mutating func mark(_ text: String, selected: NSRange? = nil) {
        marked = text
        selection = clamped(selected ?? NSRange(location: (text as NSString).length, length: 0))
    }

    public mutating func select(_ range: NSRange) {
        selection = clamped(range)
    }

    /// Text to type: a picked candidate replaces the marked text.
    public mutating func insert(_ text: String) -> String? {
        mark("")
        return text.isEmpty ? nil : text
    }

    /// The marked text, committed as it stands.
    public mutating func unmark() -> String? {
        defer { mark("") }
        return marked.isEmpty ? nil : marked
    }

    /// Text to type for `range` replaced with `text`: none while composing,
    /// where the replacement stays marked until the input method commits.
    public mutating func replace(_ range: NSRange, with text: String) -> String? {
        guard !marked.isEmpty else { return insert(text) }
        let range = clamped(range)
        mark((marked as NSString).replacingCharacters(in: range, with: text),
             selected: NSRange(location: range.location + (text as NSString).length, length: 0))
        return nil
    }

    /// `range` inside the marked text. Unlike `NSIntersectionRange`, a
    /// caret at the end stays there.
    public func clamped(_ range: NSRange) -> NSRange {
        let length = (marked as NSString).length
        let location = min(max(range.location, 0), length)
        return NSRange(location: location, length: min(max(range.length, 0), length - location))
    }

    /// The whole character next to `offset` in the marked text, before or
    /// after it; nil at either end.
    public func characterRange(from offset: Int, backward: Bool) -> NSRange? {
        let text = marked as NSString
        let offset = clamped(NSRange(location: offset, length: 0)).location
        if backward { return offset > 0 ? text.rangeOfComposedCharacterSequence(at: offset - 1) : nil }
        return offset < text.length ? text.rangeOfComposedCharacterSequence(at: offset) : nil
    }

    /// Whether the pane should get a backspace: while composing, it
    /// deletes the selection, or the character before the caret, from the
    /// marked text instead.
    public mutating func deleteBackward() -> Bool {
        guard !marked.isEmpty else { return true }
        let text = marked as NSString
        var range = selection
        if range.length == 0 {
            guard range.location > 0 else { return false }
            range = text.rangeOfComposedCharacterSequence(at: range.location - 1)
        }
        mark(text.replacingCharacters(in: range, with: ""), selected: NSRange(location: range.location, length: 0))
        return false
    }
}
