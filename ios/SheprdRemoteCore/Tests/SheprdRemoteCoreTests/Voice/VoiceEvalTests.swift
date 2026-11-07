import Foundation
import Testing
@testable import SheprdRemoteCore

/// Scores whole sentences. `tuning.json` is what the rules were written
/// against; `heldout.json` was frozen before them and `heldout2.json`
/// before the rules changed for what the first found, so the latest
/// frozen set says how they fare on phrasing nobody tuned for.
/// `transcribed.json` is what `DictationTranscriber` really returned for
/// synthesized speech, primed with the names: "work tree", "and moltis".
@Suite struct VoiceEvalTests {
    private func score(_ name: String, model: Bool) async throws -> (passed: Int, total: Int, seconds: [Double]) {
        let catalog = VoiceSample.catalog()
        let cases = try VoiceCase.load(name)
        var passed = 0
        var seconds: [Double] = []
        for item in cases {
            let started = Date()
            var draft = VoiceParser.parse(item.u, catalog: catalog)
            if model, !draft.sure, let reading = await VoiceModel.read(item.u, catalog: catalog) {
                draft = draft.adopting(reading, catalog: catalog)
            }
            seconds.append(Date().timeIntervalSince(started))
            let misses = item.misses(draft, catalog: catalog)
            if misses.isEmpty { passed += 1 } else { print("MISS [\(name)] \(item.u)\n     \(misses.joined(separator: " | "))") }
        }
        return (passed, cases.count, seconds.sorted())
    }

    @Test(arguments: ["tuning", "heldout", "heldout2", "transcribed"])
    func rulesReadEverySentence(set: String) async throws {
        let result = try await score(set, model: false)
        #expect(result.passed == result.total)
    }

    /// Runs the real model, which needs Apple Intelligence on this Mac:
    /// `VOICE_EVAL=1 swift test --filter VoiceEvalTests`.
    @Test(.enabled(if: ProcessInfo.processInfo.environment["VOICE_EVAL"] != nil))
    func report() async throws {
        for name in ["tuning", "heldout", "heldout2", "transcribed"] {
            for model in [false, true] {
                let result = try await score(name, model: model)
                let median = result.seconds[result.seconds.count / 2]
                let worst = result.seconds.last ?? 0
                print("SCORE \(name) \(model ? "rules+model" : "rules") \(result.passed)/\(result.total) median \(String(format: "%.2f", median))s worst \(String(format: "%.2f", worst))s")
            }
        }
    }
}
