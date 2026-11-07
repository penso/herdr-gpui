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
