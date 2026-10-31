import Testing
@testable import HerdrRemoteCore

@Suite struct TermInputTests {
    @Test func charactersBecomeHerdrKeyNames() {
        #expect(TermInput.sends(for: "ls -a /tmp\n") == [.keys([
            "l", "s", "space", "minus", "a", "space", "slash", "t", "m", "p", "enter",
        ])])
        #expect(TermInput.key(for: "+") == "plus")
        #expect(TermInput.key(for: "`") == "backtick")
        #expect(TermInput.key(for: "é") == "é")
    }

    @Test func aHeldModifierAppliesToTheFirstCharacterOnly() {
        #expect(TermInput.sends(for: "Cx", modifiers: .ctrl) == [.keys(["ctrl+c", "x"])])
        #expect(TermInput.sends(for: "b", modifiers: [.ctrl, .alt]) == [.keys(["ctrl+alt+b"])])
        #expect(TermInput.named("up", modifiers: .shift) == "shift+up")
    }

    @Test func controlCharactersAreDropped() {
        #expect(TermInput.sends(for: "\u{7}") == [])
        #expect(TermInput.key(for: "\u{200B}") == nil)
    }

    @Test func longTextIsPastedWhole() {
        let long = String(repeating: "word ", count: 20)
        #expect(TermInput.sends(for: long) == [.paste(long)])
    }

    @Test func longTextWithNewlinesStaysKeysInBoundedRequests() {
        // A paste would not press Enter, so lines typed stay keys.
        let text = String(repeating: "a", count: 70) + "\n"
        let sends = TermInput.sends(for: text)
        #expect(sends.count == 2)
        guard case let .keys(first) = sends[0], case let .keys(second) = sends[1] else {
            Issue.record("keys expected")
            return
        }
        #expect(first.count == TermInput.maxKeys)
        #expect(second.last == "enter")
    }
}
