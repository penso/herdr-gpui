import Foundation
import Testing
@testable import SheprdRemoteCore

@Suite struct TermScreenFillTests {
    func row(_ json: String) throws -> [TermScreen.Run] {
        try JSONDecoder().decode([TermScreen.Run].self, from: Data(json.utf8))
    }

    @Test func aBackgroundFillsTheRestOfTheLine() throws {
        #expect(TermScreen.fill(try row(#"[{"text":"status"},{"text":" ok","bg":"blue"}]"#)) == "blue")
        #expect(TermScreen.fill(try row(#"[{"text":"plain","fg":"red"}]"#)) == nil)
        #expect(TermScreen.fill([]) == nil)
    }

    @Test func reverseVideoStaysInItsOwnCells() throws {
        // A program's cursor drawn as one reversed cell at the end of its
        // input line must not paint the whole line light.
        #expect(TermScreen.fill(try row(#"[{"text":"❯ "},{"text":" ","inverse":true}]"#)) == nil)
        #expect(TermScreen.fill(try row(#"[{"text":"❯ "},{"text":" ","fg":"white","inverse":true}]"#)) == nil)
    }
}
