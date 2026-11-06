import Testing
@testable import SheprdRemoteCore

@MainActor
@Suite struct TermDictationPasteTests {
    @Test func stoppingPastesTrimmedTextOnce() async {
        let pending = TermDictationPaste()
        var pastes: [String] = []
        await pending.finish(stop: { "  git status\n" }, paste: { pastes.append($0) })
        await pending.finish(stop: { "git status" }, paste: { pastes.append($0) })
        #expect(pastes == ["git status"])
    }

    @Test func leavingDuringFinalizationDiscardsThePaste() async throws {
        let pending = TermDictationPaste()
        let (started, signal) = AsyncStream.makeStream(of: Void.self)
        var finished: CheckedContinuation<String, Never>?
        var cleanedUp = false
        var pastes: [String] = []
        let task = Task {
            await pending.finish(stop: {
                let text = await withCheckedContinuation { done in
                    finished = done
                    signal.yield(())
                    signal.finish()
                }
                cleanedUp = true
                return text
            }, paste: { pastes.append($0) })
        }
        // Leave only after stop is suspended, before its final words arrive.
        for await _ in started { break }
        pending.cancel()
        try #require(finished).resume(returning: "git status")
        await task.value
        #expect(cleanedUp)
        #expect(pastes.isEmpty)
    }

    @Test func leavingBeforeFinalizationStartsStillCleansUp() async {
        let pending = TermDictationPaste()
        var cleanedUp = false
        var pastes: [String] = []
        pending.cancel()
        await pending.finish(stop: {
            cleanedUp = true
            return "git status"
        }, paste: { pastes.append($0) })
        #expect(cleanedUp)
        #expect(pastes.isEmpty)
    }

    @Test func silenceDoesNotPaste() async {
        let pending = TermDictationPaste()
        var pastes: [String] = []
        await pending.finish(stop: { " \n " }, paste: { pastes.append($0) })
        #expect(pastes.isEmpty)
    }
}
