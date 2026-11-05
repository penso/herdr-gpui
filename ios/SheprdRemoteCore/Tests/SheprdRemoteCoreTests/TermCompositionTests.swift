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

    @Test func plainTypingPassesStraightThrough() {
        var composition = TermComposition()
        #expect(composition.insert("l") == "l")
        #expect(composition.deleteBackward() == true)
    }
}
