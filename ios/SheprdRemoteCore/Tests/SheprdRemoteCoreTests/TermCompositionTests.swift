import Foundation
import Testing
@testable import SheprdRemoteCore

@Suite struct TermCompositionTests {
    @Test func markedTextIsHeldUntilACandidateIsPicked() {
        var composition = TermComposition()
        composition.mark("ni")
        composition.mark("nihao")
        #expect(composition.marked == "nihao")
        #expect(composition.insert("你好") == "你好")
        #expect(composition.marked.isEmpty)
        #expect(TermInput.sends(for: "你好") == [.keys(["你", "好"])])
    }

    @Test func unmarkingCommitsWhatIsMarked() {
        // Some input methods mark the candidate, then unmark it.
        var composition = TermComposition()
        composition.mark("こんにちは")
        #expect(composition.unmark() == "こんにちは")
        #expect(composition.marked.isEmpty)
        #expect(composition.unmark() == nil)
    }

    @Test func clearingTheMarkedTextSendsNothing() {
        var composition = TermComposition()
        composition.mark("n")
        composition.mark("")
        #expect(composition.unmark() == nil)
    }

    @Test func backspaceWhileComposingStaysInTheComposition() {
        var composition = TermComposition()
        composition.mark("nih")
        #expect(composition.deleteBackward() == false)
        #expect(composition.marked == "ni")
        composition.mark("")
        #expect(composition.deleteBackward() == true)
    }

    @Test func rangesAreClampedToTheMarkedTextKeepingACaretAtItsEnd() {
        var composition = TermComposition()
        composition.mark("ni")
        // UIKit counts UTF-16; a caret after the last character is in range.
        #expect(composition.clamped(NSRange(location: 2, length: 0)) == NSRange(location: 2, length: 0))
        #expect(composition.clamped(NSRange(location: 1, length: 0)) == NSRange(location: 1, length: 0))
        #expect(composition.clamped(NSRange(location: 1, length: 5)) == NSRange(location: 1, length: 1))
        #expect(composition.clamped(NSRange(location: 5, length: 1)) == NSRange(location: 2, length: 0))
        #expect(composition.clamped(NSRange(location: NSNotFound, length: 0)) == NSRange(location: 2, length: 0))
        composition.mark("你好")
        #expect(composition.clamped(NSRange(location: 0, length: 9)) == NSRange(location: 0, length: 2))
    }

    @Test func markingPutsTheCaretWhereTheInputMethodSays() {
        var composition = TermComposition()
        composition.mark("ni")
        #expect(composition.selection == NSRange(location: 2, length: 0))
        composition.mark("nihao", selected: NSRange(location: 1, length: 2))
        #expect(composition.selection == NSRange(location: 1, length: 2))
        composition.select(NSRange(location: 9, length: 0))
        #expect(composition.selection == NSRange(location: 5, length: 0))
        _ = composition.insert("你好")
        #expect(composition.selection == NSRange(location: 0, length: 0))
    }

    @Test func backspaceDeletesTheSelectionOrTheCharacterBeforeTheCaret() {
        var composition = TermComposition()
        composition.mark("nihao", selected: NSRange(location: 2, length: 0))
        #expect(composition.deleteBackward() == false)
        #expect(composition.marked == "nhao")
        #expect(composition.selection == NSRange(location: 1, length: 0))

        composition.mark("nihao", selected: NSRange(location: 1, length: 3))
        #expect(composition.deleteBackward() == false)
        #expect(composition.marked == "no")
        #expect(composition.selection == NSRange(location: 1, length: 0))

        // At the start nothing is before the caret, and the pane is not
        // reached while composing.
        composition.mark("ni", selected: NSRange(location: 0, length: 0))
        #expect(composition.deleteBackward() == false)
        #expect(composition.marked == "ni")

        // A whole character, not half of a surrogate pair.
        composition.mark("a👍")
        #expect(composition.deleteBackward() == false)
        #expect(composition.marked == "a")
        #expect(composition.selection == NSRange(location: 1, length: 0))
    }

    @Test func theCharacterBesideAPositionIsOneWholeCharacter() {
        var composition = TermComposition()
        composition.mark("nihao")
        // Left of the caret after `nihao` is `o`, not the whole prefix.
        #expect(composition.characterRange(from: 5, backward: true) == NSRange(location: 4, length: 1))
        #expect(composition.characterRange(from: 2, backward: false) == NSRange(location: 2, length: 1))
        // Nothing lies past either end of the marked text.
        #expect(composition.characterRange(from: 0, backward: true) == nil)
        #expect(composition.characterRange(from: 5, backward: false) == nil)

        // A whole character, not half of a surrogate pair.
        composition.mark("a👍")
        #expect(composition.characterRange(from: 3, backward: true) == NSRange(location: 1, length: 2))
        #expect(composition.characterRange(from: 1, backward: false) == NSRange(location: 1, length: 2))
    }

    @Test func replacingPartOfTheCompositionKeepsItComposing() {
        var composition = TermComposition()
        composition.mark("nihao")
        #expect(composition.replace(NSRange(location: 0, length: 2), with: "你") == nil)
        #expect(composition.marked == "你hao")
        #expect(composition.selection == NSRange(location: 1, length: 0))
        // With nothing composed, a replacement is typed.
        composition = TermComposition()
        #expect(composition.replace(NSRange(location: 0, length: 0), with: "ok") == "ok")
    }

    @Test func plainTypingPassesStraightThrough() {
        var composition = TermComposition()
        #expect(composition.insert("l") == "l")
        #expect(composition.deleteBackward() == true)
    }
}
