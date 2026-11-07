import Foundation
import Testing
@testable import SheprdRemoteCore

struct DraftMarksTests {
    @Test func aSavedDraftIsMarkedWhenItsChatIsFirstListed() {
        var marks = DraftMarks<String>()
        marks.discover(["a", "b"]) { $0 == "a" }
        #expect(marks.marked == ["a"])
    }

    @Test func eachChatIsAskedOnce() {
        var marks = DraftMarks<String>()
        var asked: [String] = []
        for _ in 0..<2 {
            marks.discover(["a"]) { asked.append($0); return false }
        }
        #expect(asked == ["a"])
    }

    /// The file of a draft just sent may still be there: its removal is
    /// queued, and listing the chat again must not bring the mark back.
    @Test func theComposerOverrulesAFileNotRemovedYet() {
        var marks = DraftMarks<String>()
        marks.set("a", drafted: true)
        marks.set("a", drafted: false)
        marks.discover(["a"]) { _ in true }
        #expect(marks.marked.isEmpty)
    }

    @Test func aDraftTypedBeforeTheChatIsListedStaysMarked() {
        var marks = DraftMarks<String>()
        marks.set("a", drafted: true)
        marks.discover(["a"]) { _ in false }
        #expect(marks.marked == ["a"])
    }

    @Test func aForgottenChatIsAskedAboutAgain() {
        var marks = DraftMarks<String>()
        marks.set("a", drafted: true)
        marks.set("b", drafted: true)
        marks.forget { $0 == "a" }
        #expect(marks.marked == ["b"])
        marks.discover(["a"]) { _ in true }
        #expect(marks.marked == ["a", "b"])
    }
}

/// One chat in two iPad windows, each with its own composer.
struct DraftMarksWindowsTests {
    private let one = UUID()
    private let other = UUID()

    private func twoWindows() -> DraftMarks<String> {
        var marks = DraftMarks<String>()
        marks.open("a", composer: one)
        marks.open("a", composer: other)
        return marks
    }

    @Test func closingAnEmptyWindowLeavesTheOthersDraft() {
        var marks = twoWindows()
        marks.set("a", drafted: true, by: one)
        let removes = marks.set("a", drafted: false, by: other)
        marks.close("a", composer: other)
        #expect(!removes)
        #expect(marks.marked == ["a"])
    }

    @Test func anEmptyWindowClosedLastKeepsTheDraftSavedByTheFirst() {
        var marks = twoWindows()
        marks.set("a", drafted: true, by: one)
        marks.close("a", composer: one)
        let removes1 = marks.set("a", drafted: false, by: other)
        #expect(!removes1)
        #expect(marks.marked == ["a"])
    }

    @Test func emptyingOneOfTwoDraftsKeepsTheMark() {
        var marks = twoWindows()
        marks.set("a", drafted: true, by: one)
        marks.set("a", drafted: true, by: other)
        let removes2 = marks.set("a", drafted: false, by: one)
        #expect(!removes2)
        #expect(marks.marked == ["a"])
        let removes3 = marks.set("a", drafted: false, by: other)
        #expect(removes3)
        #expect(marks.marked.isEmpty)
    }

    /// Typing clears the mark first, and leaving saves nothing afterwards:
    /// that save must still remove the file.
    @Test func aComposerEmptiedByHandStillRemovesTheFileOnLeaving() {
        var marks = DraftMarks<String>()
        marks.open("a", composer: one)
        marks.set("a", drafted: true, by: one)
        marks.set("a", drafted: false, by: one)
        let removes4 = marks.set("a", drafted: false, by: one)
        #expect(removes4)
    }

    /// A failed send comes back after its composer closed: it stays marked,
    /// and a later composer can clear it.
    @Test func aSaveAfterClosingLeavesNoHolderBehind() {
        var marks = DraftMarks<String>()
        marks.open("a", composer: one)
        marks.close("a", composer: one)
        marks.set("a", drafted: true, by: one)
        #expect(marks.marked == ["a"])
        marks.open("a", composer: other)
        marks.set("a", drafted: true, by: other)
        let removes5 = marks.set("a", drafted: false, by: other)
        #expect(removes5)
        #expect(marks.marked.isEmpty)
    }

    @Test func aDraftSavedWithNoComposerCanBeCleared() {
        var marks = DraftMarks<String>()
        marks.set("a", drafted: true)
        let removes6 = marks.set("a", drafted: false)
        #expect(removes6)
        #expect(marks.marked.isEmpty)
    }
}
