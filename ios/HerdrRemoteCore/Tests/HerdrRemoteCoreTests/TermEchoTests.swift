import Testing
@testable import HerdrRemoteCore

struct TermEchoTests {
    let start = ContinuousClock.now

    func at(_ milliseconds: Int) -> TermEcho.Instant { start + .milliseconds(milliseconds) }

    @Test func keepsTheLineWhileTypingAndClearsAfterAPause() {
        var echo = TermEcho()
        echo.queue(.keys(["l", "s", "space", "minus", "l", "enter"]), at: at(0))
        #expect(echo.text == "ls -l⏎")
        echo.sending(6, at: at(0))
        echo.delivered(at: at(40))
        #expect(echo.allTaken)
        // Taken, but typing paused only just now: the line stays whole.
        echo.settle(at: at(1000))
        #expect(echo.text == "ls -l⏎")
        echo.settle(at: at(2000))
        #expect(echo.text.isEmpty)
    }

    @Test func typingAgainKeepsWhatWasTakenOnTheLine() {
        var echo = TermEcho()
        echo.queue(.keys(["a"]), at: at(0))
        echo.sending(1, at: at(0))
        echo.delivered(at: at(40))
        echo.queue(.keys(["b"]), at: at(1500))
        echo.settle(at: at(2040))
        #expect(echo.text == "ab")
    }

    @Test func aLineWithInputNotTakenYetNeverClears() {
        var echo = TermEcho()
        echo.queue(.keys(["a"]), at: at(0))
        echo.sending(1, at: at(0))
        #expect(!echo.allTaken)
        echo.settle(at: at(10_000))
        #expect(echo.text == "a")
        echo.delivered(at: at(10_040))
        echo.settle(at: at(10_040))
        #expect(echo.text.isEmpty)
    }

    @Test func backspaceTakesBackWhatItCanAndMarksTheRest() {
        var echo = TermEcho()
        echo.queue(.keys(["a", "b", "backspace"]), at: at(0))
        #expect(echo.text == "a")
        echo.queue(.keys(["backspace", "backspace", "ctrl+c", "up"]), at: at(10))
        #expect(echo.text == "⌫")
    }

    @Test func aFailedRequestDropsEverything() {
        var echo = TermEcho()
        echo.queue(.keys(["a"]), at: at(0))
        echo.queue(.paste("long text"), at: at(0))
        echo.queue(.keys(["b"]), at: at(0))
        #expect(echo.text == "ab")
        echo.sending(1, at: at(0))
        echo.clear()
        #expect(echo.text.isEmpty)
    }
}
