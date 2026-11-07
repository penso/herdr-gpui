import SheprdRemoteCore
import UIKit

/// An offset into the marked text, in UTF-16 as UIKit counts.
final class TermTextPosition: UITextPosition {
    let offset: Int
    init(_ offset: Int) { self.offset = offset }
}

final class TermTextRange: UITextRange {
    let range: NSRange
    init(_ range: NSRange) { self.range = range }
    override var start: UITextPosition { TermTextPosition(range.location) }
    override var end: UITextPosition { TermTextPosition(NSMaxRange(range)) }
    override var isEmpty: Bool { range.length == 0 }
}

/// The document an input method sees is only its marked text: what was
/// committed is already in the pane, which the phone cannot edit.
extension TermInputView: UITextInput {
    private var marked: NSString { composition.marked as NSString }

    private func clamped(_ range: NSRange) -> NSRange { composition.clamped(range) }

    // MARK: Marked text

    var markedTextRange: UITextRange? {
        composition.marked.isEmpty ? nil : TermTextRange(NSRange(location: 0, length: marked.length))
    }

    func setMarkedText(_ markedText: String?, selectedRange: NSRange) {
        composition.mark(markedText ?? "", selected: selectedRange)
    }

    func unmarkText() {
        // Some input methods commit this way. One interrupted (the keyboard
        // hidden, the app inactive under a screenshot) has picked nothing,
        // and its letters are not typed.
        guard let text = composition.unmark(), isFirstResponder, UIApplication.shared.applicationState == .active else { return }
        typing?.type(text)
    }

    func text(in range: UITextRange) -> String? {
        guard let range = range as? TermTextRange else { return nil }
        return marked.substring(with: clamped(range.range))
    }

    func replace(_ range: UITextRange, withText text: String) {
        guard let range = range as? TermTextRange else { return }
        if let text = composition.replace(range.range, with: text) { typing?.type(text) }
    }

    var selectedTextRange: UITextRange? {
        get { TermTextRange(composition.selection) }
        set { if let range = newValue as? TermTextRange { composition.select(range.range) } }
    }

    // MARK: Positions

    var beginningOfDocument: UITextPosition { TermTextPosition(0) }
    var endOfDocument: UITextPosition { TermTextPosition(marked.length) }

    func textRange(from fromPosition: UITextPosition, to toPosition: UITextPosition) -> UITextRange? {
        guard let from = fromPosition as? TermTextPosition, let to = toPosition as? TermTextPosition else { return nil }
        return TermTextRange(NSRange(location: min(from.offset, to.offset), length: abs(to.offset - from.offset)))
    }

    func position(from position: UITextPosition, offset: Int) -> UITextPosition? {
        guard let position = position as? TermTextPosition else { return nil }
        let moved = position.offset + offset
        return (0...marked.length).contains(moved) ? TermTextPosition(moved) : nil
    }

    func position(from position: UITextPosition, in direction: UITextLayoutDirection, offset: Int) -> UITextPosition? {
        switch direction {
        case .left, .up: self.position(from: position, offset: -offset)
        default: self.position(from: position, offset: offset)
        }
    }

    func compare(_ position: UITextPosition, to other: UITextPosition) -> ComparisonResult {
        let lhs = (position as? TermTextPosition)?.offset ?? 0
        let rhs = (other as? TermTextPosition)?.offset ?? 0
        return lhs < rhs ? .orderedAscending : lhs > rhs ? .orderedDescending : .orderedSame
    }

    func offset(from: UITextPosition, to toPosition: UITextPosition) -> Int {
        ((toPosition as? TermTextPosition)?.offset ?? 0) - ((from as? TermTextPosition)?.offset ?? 0)
    }

    func position(within range: UITextRange, farthestIn direction: UITextLayoutDirection) -> UITextPosition? {
        switch direction {
        case .left, .up: range.start
        default: range.end
        }
    }

    func characterRange(byExtending position: UITextPosition, in direction: UITextLayoutDirection) -> UITextRange? {
        guard let position = position as? TermTextPosition else { return nil }
        let backward = direction == .left || direction == .up
        return composition.characterRange(from: position.offset, backward: backward).map(TermTextRange.init)
    }

    func baseWritingDirection(for position: UITextPosition, in direction: UITextStorageDirection) -> NSWritingDirection {
        .natural
    }

    func setBaseWritingDirection(_ writingDirection: NSWritingDirection, for range: UITextRange) {}

    // MARK: Geometry

    // The view is a hidden point; candidates shown beside the text (a
    // hardware keyboard on iPad) appear there.
    func firstRect(for range: UITextRange) -> CGRect { bounds }
    func caretRect(for position: UITextPosition) -> CGRect { bounds }
    func selectionRects(for range: UITextRange) -> [UITextSelectionRect] { [] }
    func closestPosition(to point: CGPoint) -> UITextPosition? { endOfDocument }
    func closestPosition(to point: CGPoint, within range: UITextRange) -> UITextPosition? { range.end }
    func characterRange(at point: CGPoint) -> UITextRange? { nil }
}
