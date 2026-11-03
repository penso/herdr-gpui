import Foundation
import Testing
@testable import HerdrRemoteCore

/// The daemon's Rust tests write these fixtures from its own types
/// (`native/tests/fixtures.rs`); decoding them here keeps the two sides in
/// step.
struct WireTests {
    static func fixture(_ name: String) throws -> Data {
        let url = try #require(Bundle.module.url(forResource: "Fixtures/v2/\(name)", withExtension: "json"))
        return try Data(contentsOf: url)
    }

    static func event(_ name: String) throws -> LiveEvent {
        try JSONDecoder().decode(LiveEvent.self, from: fixture(name))
    }

    @Test func decodesHome() throws {
        guard case let .home(home) = try Self.event("home") else { Issue.record("not home"); return }
        #expect(home.computer == "MacBook Pro")
        #expect(home.waiting == 1)
        let workspace = try #require(home.workspaces.first)
        #expect(workspace.pullRequest?.number == 870)
        #expect(workspace.pullRequest?.tone == .waiting)
        let chat = try #require(workspace.chats.first)
        #expect(chat.status == .working)
        #expect(chat.summary?.question?.options.first?.kind == .allowOnce)
        #expect(chat.summary?.usagePercent == 42)
        #expect(workspace.chats[1].summary == nil)
        #expect(workspace.chats[1].agent == nil)
        #expect(workspace.terminals.first?.firstPane?.pane == "p2")
        #expect(workspace.terminals.first?.agent?.status == .done)
        #expect(home.workspaces[1].pullRequest == nil)
    }

    @Test func decodesChatSnapshotAndTheRestEndpoint() throws {
        guard case let .chatSnapshot(snapshot) = try Self.event("chat_snapshot") else { Issue.record("not a snapshot"); return }
        #expect(snapshot.cursor?.seq == 41)
        #expect(snapshot.transcript.entries.count == 2)
        #expect(snapshot.transcript.permissions.first?.id == 3)
        #expect(snapshot.transcript.usage?.percent == 42)
        let rest = try JSONDecoder().decode(ChatSnapshot.self, from: Self.fixture("chat"))
        #expect(rest.pane == snapshot.pane)
    }

    @Test func decodesChangesAndTheirCursors() throws {
        guard case let .chatChanges(pane, changes) = try Self.event("chat_changes") else { Issue.record("not changes"); return }
        #expect(pane == "p1")
        #expect(changes.map(\.cursor.seq) == [42, 43, 44])
        #expect(changes[0].change == .append(id: 2, text: " Tests cover nested splits."))
        #expect(changes[2].change == .status(.idle))
        #expect(changes[0].cursor.isFollowed(by: changes[1].cursor))
    }

    @Test func decodesTheSmallMessages() throws {
        guard case let .chatResumed(_, cursor, connected) = try Self.event("chat_resumed") else { Issue.record("resumed"); return }
        #expect(cursor.seq == 41 && connected)
        #expect(try Self.event("chat_connection") == .chatConnection(pane: "p1", connected: false))
        #expect(try Self.event("reload") == .reload(.chat(pane: "p1")))
        #expect(try Self.event("error") == .error("no such chat"))
        #expect(try Self.event("bye") == .bye(reason: "revoked"))
    }

    @Test func decodesTerminalRuns() throws {
        guard case let .termScreen(screen) = try Self.event("term_screen") else { Issue.record("not a screen"); return }
        #expect(screen.columns == 21)
        let first = try #require(screen.rows.first?.first)
        #expect(first.text == "~/code" && first.fg == "cyan" && !first.bold)
        let runs = screen.rows.flatMap { $0 }
        #expect(runs.contains { $0.fg == "#ff8000" })
        #expect(runs.contains { $0.inverse })
        #expect(runs.contains { $0.italic && $0.underline })
        #expect(runs.contains { $0.dim })
    }

    /// The phone's messages must be what the daemon reads, key for key.
    @Test(arguments: [
        ("subscribe_home", LiveRequest.subscribe(.home)),
        ("subscribe_term", LiveRequest.subscribe(.term(pane: "p2"))),
        ("unsubscribe_chat", LiveRequest.unsubscribe(.chat(pane: "p1"))),
        (
            "subscribe_chat",
            LiveRequest.subscribe(
                .chat(pane: "p1"),
                since: Cursor(epoch: UUID(uuidString: "01234567-89ab-cdef-0123-456789abcdef")!, seq: 41)
            )
        ),
    ])
    func encodesRequestsAsTheDaemonReadsThem(name: String, request: LiveRequest) throws {
        let expected = try JSONSerialization.jsonObject(with: Self.fixture(name)) as? NSDictionary
        let encoder = JSONEncoder()
        let actual = try JSONSerialization.jsonObject(with: encoder.encode(request)) as? NSDictionary
        // UUIDs encode uppercase in Swift and lowercase in Rust; both parse.
        #expect(normalized(actual) == normalized(expected))
    }

    private func normalized(_ dictionary: NSDictionary?) -> String {
        let data = (try? JSONSerialization.data(withJSONObject: dictionary ?? [:], options: .sortedKeys)) ?? Data()
        return String(decoding: data, as: UTF8.self).lowercased()
    }
}

/// Every vector the Rust reducer produced must come out the same here.
struct ReducerTests {
    struct Vector: Decodable {
        let start: Transcript
        let changes: [Change]
        let end: Transcript
        let error: String?
    }

    static let names = [
        "entries", "permissions", "session",
        "rejected_duplicate_permission", "rejected_entry_order", "rejected_no_modes",
        "rejected_not_text", "rejected_unknown_entry", "rejected_unknown_permission",
    ]

    @Test(arguments: names)
    func matchesTheRustReducer(name: String) throws {
        let vector = try JSONDecoder().decode(Vector.self, from: WireTests.fixture("reducer/\(name)"))
        var transcript = vector.start
        var failure: ChangeError?
        for (index, change) in vector.changes.enumerated() {
            do {
                try transcript.apply(change)
            } catch {
                failure = error
                #expect(index == vector.changes.count - 1, "only the last change may fail")
            }
        }
        #expect(failure?.rawValue == vector.error)
        #expect(transcript == vector.end)
    }

    @Test func refusesTooManyQuestionAnswers() throws {
        var transcript = Transcript(agent: "Claude")
        let answers = Array(repeating: QuestionAnswer(question: "q", answer: "a"), count: QuestionAnswer.maxCount + 1)
        var notice = try JSONDecoder().decode(Notice.self, from: Data(#"{"level":"info","text":"t"}"#.utf8))
        notice.answers = answers
        #expect(throws: ChangeError.questionAnswersLimit) {
            try transcript.apply(.push(Entry(id: 1, kind: .notice(notice))))
        }
        #expect(transcript.entries.isEmpty)
    }

    /// A cached transcript survives a round trip through its own encoding.
    @Test func roundTripsForTheCache() throws {
        let vector = try JSONDecoder().decode(Vector.self, from: WireTests.fixture("reducer/entries"))
        let data = try JSONEncoder().encode(vector.end)
        #expect(try JSONDecoder().decode(Transcript.self, from: data) == vector.end)
    }
}
