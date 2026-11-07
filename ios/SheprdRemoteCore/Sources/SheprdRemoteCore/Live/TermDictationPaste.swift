import Foundation

/// One request to finish dictating and paste into a terminal. Leaving the
/// terminal cancels the paste synchronously, while transcription can still
/// finish and release the microphone.
@MainActor
public final class TermDictationPaste {
    private var cancelled = false

    public init() {}

    public func cancel() {
        cancelled = true
    }

    public func finish(stop: () async -> String, paste: (String) -> Void) async {
        let text = await stop()
        guard !cancelled else { return }
        cancelled = true
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        if !trimmed.isEmpty { paste(trimmed) }
    }
}
