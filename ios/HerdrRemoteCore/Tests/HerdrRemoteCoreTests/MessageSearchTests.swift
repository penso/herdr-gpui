import Foundation
import Testing
@testable import HerdrRemoteCore

struct MessageSearchTests {
    func transcript(_ kinds: [EntryKind]) -> Transcript {
        var transcript = Transcript(agent: "Claude")
        transcript.entries = kinds.enumerated().map { Entry(id: EntryId(UInt64($0.offset)), kind: $0.element) }
        return transcript
    }

    @Test func aQueryIsOneLineAndWithinBounds() {
        #expect(MessageText.query("  the \n splitter ") == "the splitter")
        #expect(MessageText.query(" a ") == nil)
        #expect(MessageText.query(String(repeating: "x", count: 101)) == nil)
        #expect(MessageText.fold("Fix  the\nSplitter") == "fix the splitter")
    }

    @Test func aSnippetKeepsTheWordsAroundTheMatch() {
        #expect(MessageText.snippet("short\nanswer", around: "ANSWER") == "short answer")
        let text = String(repeating: "a", count: 100) + "needle" + String(repeating: " b", count: 200)
        let snippet = MessageText.snippet(text, around: "Needle")
        #expect(snippet.hasPrefix("…"))
        #expect(snippet.hasSuffix("…"))
        #expect(snippet.count == 160)
        #expect(snippet.dropFirst(41).hasPrefix("needle"))
    }

    @Test func theComputersAnswerDecodes() throws {
        let json = #"{"hits":[{"pane":"p1","entry":42,"author":"agent","snippet":"…the splitter"}],"truncated":true}"#
        let answer = try JSONDecoder().decode(SearchAnswer.self, from: Data(json.utf8))
        #expect(answer.hits == [SearchHit(pane: "p1", entry: 42, author: .agent, snippet: "…the splitter")])
        #expect(answer.truncated)
    }

    @Test func theIndexFindsMessagesAndKeepsOlderOnes() async throws {
        let index = MessageIndex(url: nil)
        let first = transcript([
            .user(text: "Rename the Splitter", files: 0),
            .thought(text: "the splitter is in layout.rs"),
            .agent(text: "Renamed the splitter."),
        ])
        try await index.save(first, device: "mac", pane: "p1")
        // A later transcript that no longer carries the first entries, and
        // changed the answer as it streamed.
        var later = transcript([])
        later.entries = [
            Entry(id: 2, kind: .agent(text: "Renamed the splitter, and its tests.")),
            Entry(id: 3, kind: .user(text: "now the divider", files: 0)),
        ]
        try await index.save(later, device: "mac", pane: "p1")
        try await index.save(transcript([.agent(text: "splitter elsewhere")]), device: "linux", pane: "p9")

        let matches = try await index.search("the  SPLITTER", devices: ["mac"])
        #expect(Set(matches.map(\.hit.entry)) == [0, 2])
        #expect(matches.allSatisfy { $0.device == "mac" && $0.hit.pane == "p1" })
        #expect(matches.contains { $0.hit.snippet == "Renamed the splitter, and its tests." && $0.hit.author == .agent })
        // Thoughts are not messages.
        #expect(try await index.search("layout.rs", devices: ["mac"]).isEmpty)
        #expect(try await index.search("splitter", devices: ["mac", "linux"], perChat: 1).count == 2)

        try await index.forget(device: "mac")
        #expect(try await index.search("splitter", devices: ["mac"]).isEmpty)
        #expect(try await index.search("splitter", devices: ["linux"]).count == 1)
    }

    @Test func theIndexSurvivesReopeningAndAStoreItCannotRead() async throws {
        let folder = FileManager.default.temporaryDirectory.appending(path: UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: folder) }
        let url = folder.appending(path: "Messages.sqlite")
        try await MessageIndex(url: url).save(transcript([.user(text: "keep this", files: 0)]), device: "mac", pane: "p1")
        #expect(try await MessageIndex(url: url).search("keep", devices: ["mac"]).count == 1)
        #expect(MessageIndex(url: url).size() > 0)

        try FileManager.default.removeItem(at: folder)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        try Data("not a database".utf8).write(to: url)
        let fresh = MessageIndex(url: url)
        #expect(try await fresh.search("keep", devices: ["mac"]).isEmpty)
        try await fresh.save(transcript([.user(text: "keep this", files: 0)]), device: "mac", pane: "p1")
        #expect(try await fresh.search("keep", devices: ["mac"]).count == 1)
    }
}
