import Testing
@testable import SheprdRemoteCore

@Suite struct TermCellsTests {
    @Test func latinTakesACellEach() {
        #expect(TermScreen.cells("❯ hello") == 7)
        #expect(TermScreen.cells("") == 0)
    }

    @Test func chineseJapaneseAndKoreanTakeTwo() {
        // A program's cursor after them is drawn where the terminal put it.
        #expect(TermScreen.cells("好的回电话") == 10)
        #expect(TermScreen.cells("かなカナ한글") == 12)
        #expect(TermScreen.cells("，。\u{3000}") == 6)
        #expect(TermScreen.cells("ＡＢ") == 4)
    }

    @Test func emojiTakeTwo() {
        #expect(TermScreen.cells("👍") == 2)
        #expect(TermScreen.cells("❤️") == 2)
        #expect(TermScreen.cells("👨‍👩‍👧") == 2)
    }

    @Test func boxDrawingAndAccentsTakeOne() {
        #expect(TermScreen.cells("─│█") == 3)
        #expect(TermScreen.cells("é") == 1)
        #expect(TermScreen.cells("❯") == 1)
    }
}
