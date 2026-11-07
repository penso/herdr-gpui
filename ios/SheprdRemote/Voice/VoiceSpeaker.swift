import AVFoundation

/// Says short results and questions aloud, unless turned off in Settings.
@MainActor
final class VoiceSpeaker: NSObject, AVSpeechSynthesizerDelegate {
    static let setting = "voiceSpeaks"

    private let synthesizer = AVSpeechSynthesizer()
    private var finished: CheckedContinuation<Void, Never>?

    override init() {
        super.init()
        synthesizer.delegate = self
    }

    static var enabled: Bool {
        UserDefaults.standard.object(forKey: setting) as? Bool ?? true
    }

    /// Returns once it has been said, so the microphone does not hear it.
    func say(_ words: String) async {
        guard Self.enabled else { return }
        stop()
        let session = AVAudioSession.sharedInstance()
        try? session.setCategory(.playback, options: .duckOthers)
        try? session.setActive(true)
        await withCheckedContinuation { continuation in
            finished = continuation
            synthesizer.speak(AVSpeechUtterance(string: words))
        }
        try? session.setActive(false, options: .notifyOthersOnDeactivation)
    }

    func stop() {
        synthesizer.stopSpeaking(at: .immediate)
        resume()
    }

    private func resume() {
        finished?.resume()
        finished = nil
    }

    nonisolated func speechSynthesizer(_ synthesizer: AVSpeechSynthesizer, didFinish utterance: AVSpeechUtterance) {
        Task { @MainActor in self.resume() }
    }

    nonisolated func speechSynthesizer(_ synthesizer: AVSpeechSynthesizer, didCancel utterance: AVSpeechUtterance) {
        Task { @MainActor in self.resume() }
    }
}
