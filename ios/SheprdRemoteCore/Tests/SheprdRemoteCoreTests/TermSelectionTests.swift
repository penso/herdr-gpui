import Foundation
import Testing
@testable import SheprdRemoteCore

@Suite struct TermSelectionTests {
    func screen(_ rows: [String]) throws -> TermScreen {
        let json = rows.map { #"[{"text":"\#($0)"}]"# }.joined(separator: ",")
        return try JSONDecoder().decode(TermScreen.self, from: Data(#"{"pane":"p","columns":40,"rows":[\#(json)]}"#.utf8))
    }

    func at(_ row: Int, _ column: Int) -> TermSelection.Position { .init(row: row, column: column) }

    @Test func aLongPressSelectsTheWordUnderIt() throws {
        let screen = try screen(["ls -la /tmp"])
        let word = screen.word(at: at(0, 4))
        #expect(word.start == at(0, 3) && word.end == at(0, 5))
        #expect(screen.text(word) == "-la")
    }

    @Test func chineseIsSelectedByItsCells() throws {
        // ❯ and a space, then five characters two cells each.
        let screen = try screen(["❯ 好的回电话 ok"])
        let word = screen.word(at: at(0, 5))
        #expect(word.start == at(0, 2) && word.end == at(0, 11))
        #expect(screen.text(word) == "好的回电话")
        // Half of a character selects all of it.
        #expect(screen.text(TermSelection(anchor: at(0, 3), head: at(0, 4))) == "好的")
    }

    @Test func aBlankCellIsSelectedAlone() throws {
        let screen = try screen(["a  b"])
        let blank = screen.word(at: at(0, 1))
        #expect(blank.start == at(0, 1) && blank.end == at(0, 1))
        #expect(screen.word(at: at(0, 30)).start == at(0, 30))
    }

    @Test func draggingBackSelectsInReadingOrder() throws {
        let screen = try screen(["first line   ", "second"])
        let dragged = TermSelection(anchor: at(1, 2), head: at(0, 6))
        #expect(dragged.start == at(0, 6) && dragged.end == at(1, 2))
        // Rows keep their own text, without the blanks that end them.
        #expect(screen.text(dragged) == "line\nsec")
        #expect(dragged.columns(in: 0, last: 12) == 6...12)
        #expect(dragged.columns(in: 1, last: 5) == 0...2)
        #expect(dragged.columns(in: 2, last: 5) == nil)
    }

    @Test func selectAllCoversTheScreen() throws {
        let screen = try screen(["one", "two words"])
        #expect(screen.text(screen.everything) == "one\ntwo words")
    }

    @Test func hiddenTextIsCopiedBlank() throws {
        let screen = try JSONDecoder().decode(TermScreen.self, from: Data(#"""
            {"pane":"p","columns":20,"rows":[[{"text":"pw: "},{"text":"secret","hidden":true},{"text":" ok"}]]}
            """#.utf8))
        #expect(screen.text(screen.everything) == "pw:        ok")
    }
}
