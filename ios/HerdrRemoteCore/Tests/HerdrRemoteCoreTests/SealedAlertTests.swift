import CryptoKit
import Foundation
import Testing
@testable import HerdrRemoteCore

@Suite struct SealedAlertTests {
    static func alert(_ json: String) throws -> SealedAlert {
        try JSONDecoder().decode(SealedAlert.self, from: Data(json.utf8))
    }

    static let question = #"""
    {"v":1,"kind":"question","clear":false,"computer":"AB","pane":"w1:p2","title":"Claude in app","body":"Run git push?","badge":2,
     "permission":{"id":7,"options":[
       {"id":"allow","name":"Allow once","kind":"allow_once"},
       {"id":"always","name":"Always allow","kind":"allow_always"},
       {"id":"deny","name":"Deny","kind":"reject_once"},
       {"id":"x","name":"Other","kind":"something_new"},
       {"id":"fifth","name":"Fifth","kind":"other"}]}}
    """#

    @Test func aQuestionOffersItsFirstFourAnswers() throws {
        let presentation = try Self.alert(Self.question).presentation
        #expect(presentation.actions.map(\.id) == ["answer:allow", "answer:always", "answer:deny", "answer:x"])
        #expect(presentation.actions.map(\.style) == [.plain, .authenticated, .destructive, .plain])
        #expect(presentation.category?.hasPrefix("question.") == true)
    }

    @Test func theSameAnswersShareACategory() throws {
        let one = try Self.alert(Self.question).presentation.category
        let two = try Self.alert(Self.question.replacingOccurrences(of: "\"id\":7", with: "\"id\":8")).presentation.category
        let other = try Self.alert(Self.question.replacingOccurrences(of: "Deny", with: "No")).presentation.category
        #expect(one == two)
        #expect(one != other)
    }

    @Test func aFinishedTurnOffersReplyAndAnErrorNothing() throws {
        let finished = try Self.alert(#"{"kind":"turn_ended","computer":"AB","pane":"p","title":"t","body":"b"}"#).presentation
        #expect(finished.category == AlertPresentation.finishedCategory)
        #expect(finished.actions.map(\.id) == ["reply"])
        let failed = try Self.alert(#"{"kind":"error","computer":"AB","pane":"p","title":"t","body":"b"}"#).presentation
        #expect(failed.category == nil && failed.actions.isEmpty)
    }

    @Test func actionsBecomeChatCommands() {
        #expect(AlertPresentation.command(action: "answer:allow", permission: 7, text: nil) == .answer(permission: 7, option: "allow"))
        #expect(AlertPresentation.command(action: "answer:allow", permission: nil, text: nil) == nil)
        #expect(AlertPresentation.command(action: "reply", permission: nil, text: "  ship it \n") == .prompt(text: "ship it"))
        #expect(AlertPresentation.command(action: "reply", permission: nil, text: "   ") == nil)
        #expect(AlertPresentation.command(action: "com.apple.UNNotificationDefaultActionIdentifier", permission: 7, text: nil) == nil)
    }
}

/// The daemon's own sealed alert (`Fixtures/notify/alert.json`, written by
/// `apns/tests/alert.rs`) opens here to the plaintext it sealed.
@Suite struct SealedAlertFixtureTests {
    struct Fixture: Decodable {
        let device_private_key: String
        let apns: APNs
        struct APNs: Decodable { let sealed: String }
    }

    @Test func theDaemonsAlertOpensWithTheDeviceKey() throws {
        let url = try #require(Bundle.module.url(forResource: "Fixtures/notify/alert", withExtension: "json"))
        let data = try Data(contentsOf: url)
        let fixture = try JSONDecoder().decode(Fixture.self, from: data)
        let key = try P256.KeyAgreement.PrivateKey(rawRepresentation: #require(Base64URL.decode(fixture.device_private_key)))
        let alert = try SealedAlert.open(fixture.apns.sealed, device: key.publicKey) { try key.sharedSecretFromKeyAgreement(with: $0) }
        let raw = try #require(JSONSerialization.jsonObject(with: data) as? [String: Any])
        let plaintext = try #require(raw["plaintext"] as? [String: Any])
        #expect(alert.pane == plaintext["pane"] as? String)
        #expect(alert.title == plaintext["title"] as? String)
        #expect(alert.computer == plaintext["computer"] as? String)
        if let permission = plaintext["permission"] as? [String: Any] {
            #expect(alert.permission?.id.value == (permission["id"] as? NSNumber)?.uint64Value)
            #expect(alert.permission?.options.count == (permission["options"] as? [Any])?.count)
        }
        #expect(!alert.presentation.actions.isEmpty || alert.kind != .question)
    }
}

@Suite struct ActivityAnswerTests {
    static func transcript(title: String) throws -> Transcript {
        let json = """
        {"agent":"claude","permissions":[{"id":7,"call_id":"c","title":\(String(reflecting: title)),
          "options":[{"id":"allow","kind":"allow_once","name":"Allow once"},
                     {"id":"always","kind":"allow_always","name":"Always allow"}]}]}
        """
        return try JSONDecoder().decode(Transcript.self, from: Data(json.utf8))
    }

    static let allow = ChatActivityState.Option(id: "allow", name: "Allow once", kind: .allowOnce)

    @Test func theSameQuestionAndAnswerOnTheNamedChatMatch() throws {
        let chat = try Self.transcript(title: "Run  git push\norigin main")
        #expect(ActivityAnswer.matches(question: "Run git push origin main", permission: 7, option: Self.allow, in: chat))
    }

    @Test func aLongQuestionMatchesOnlyTheDaemonsExactCut() throws {
        let title = String(repeating: "word ", count: 100)
        let chat = try Self.transcript(title: title)
        let cut = ActivityAnswer.excerpt(title)
        #expect(cut.unicodeScalars.count <= ActivityAnswer.excerptLength)
        #expect(cut.hasSuffix("word\u{2026}"))
        #expect(ActivityAnswer.matches(question: cut, permission: 7, option: Self.allow, in: chat))
        // A shorter prefix is not what the daemon would have shown.
        #expect(!ActivityAnswer.matches(question: "word\u{2026}", permission: 7, option: Self.allow, in: chat))
    }

    @Test func excerptsMatchTheDaemons() {
        // As `push/tests/triggers.rs` checks the Rust side.
        #expect(ActivityAnswer.excerpt("  a\n\nb\tc  ") == "a b c")
        let exact = String(repeating: "x", count: ActivityAnswer.excerptLength)
        #expect(ActivityAnswer.excerpt(exact) == exact)
    }

    @Test func anotherQuestionIdOrAnswerDoesNot() throws {
        let chat = try Self.transcript(title: "Run git push origin main")
        let question = "Run git push origin main"
        #expect(!ActivityAnswer.matches(question: "Delete the repo", permission: 7, option: Self.allow, in: chat))
        #expect(!ActivityAnswer.matches(question: question, permission: 8, option: Self.allow, in: chat))
        // The real "always" answer, relabelled as a one-time one.
        let disguised = ChatActivityState.Option(id: "always", name: "Allow once", kind: .allowOnce)
        #expect(!ActivityAnswer.matches(question: question, permission: 7, option: disguised, in: chat))
        let honest = ChatActivityState.Option(id: "always", name: "Always allow", kind: .allowAlways)
        #expect(!ActivityAnswer.matches(question: question, permission: 7, option: honest, in: chat))
    }
}
