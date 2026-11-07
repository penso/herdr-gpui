import Testing
@testable import SheprdRemoteCore

@Suite struct VoiceNamesTests {
    private func heard(_ name: String, in text: String, partly: Bool = false) -> Bool {
        VoiceNames.find(name, in: VoiceNames.words(text), partly: partly) != nil
    }

    @Test func spokenDigitsJoinIntoOneWord() {
        #expect(VoiceNames.words("build oh one now").map(\.text) == ["build", "01", "now"])
        // A lone number word stays a word.
        #expect(VoiceNames.words("oh that one").map(\.text) == ["oh", "that", "one"])
    }

    @Test(arguments: [
        ("sheprd", "go to shepherd"),
        ("sheprd", "the shepard repo"),
        ("herdr", "open herder"),
        ("build-01", "on build oh one"),
        ("build-01", "on build 01"),
        ("studio", "on studios"),
    ])
    func namesSurviveTheTranscriber(name: String, text: String) {
        #expect(heard(name, in: text))
    }

    @Test(arguments: [("site", "the light"), ("studio", "on stadium"), ("moltis", "the motor"), ("sheprd", "the shop")])
    func unrelatedWordsAreNotNames(name: String, text: String) {
        #expect(!heard(name, in: text))
    }

    @Test func partOfALongNameCountsOnlyWhenAsked() {
        #expect(heard("MacBook Pro", in: "on the macbook", partly: true))
        #expect(!heard("MacBook Pro", in: "on the macbook"))
        // Not by a word many titles share.
        #expect(!heard("Fix mise discovery on SSH", in: "fix it", partly: true))
    }
}

@Suite struct VoiceTranscriberWordsTests {
    @Test func aWorktreeIsOneWordHoweverItIsWritten() {
        #expect(VoiceNames.words("a work tree on studio").map(\.text) == ["a", "worktree", "on", "studio"])
        #expect(VoiceNames.words("new work space").map(\.text) == ["new", "workspace"])
        // The merged word keeps the speaker's span.
        let text = "a work tree"
        let word = VoiceNames.words(text)[1]
        #expect(text[word.range] == "work tree")
    }
}
