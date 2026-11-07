import AVFoundation

/// The choices in Settings > Voice.
enum VoiceSettings {
    static let speaksKey = "voiceSpeaks"
    static let confirmsKey = "voiceConfirms"
    static let languageKey = "voicePhoneLanguage"

    static var speaks: Bool { UserDefaults.standard.object(forKey: speaksKey) as? Bool ?? true }
    /// A spoken yes starts a new worktree, for use without looking.
    static var confirms: Bool { UserDefaults.standard.bool(forKey: confirmsKey) }
    /// Commands in the phone's language rather than English. Only the
    /// model can read those, and less reliably.
    static var phoneLanguage: Bool { UserDefaults.standard.bool(forKey: languageKey) }
    static var english: Bool { !phoneLanguage || Locale.current.language.languageCode == .english }
}

/// Says short results and questions aloud, unless turned off in Settings.
@MainActor
final class VoiceSpeaker: NSObject, AVSpeechSynthesizerDelegate {
    private let synthesizer = AVSpeechSynthesizer()
    private var finished: CheckedContinuation<Void, Never>?

    override init() {
        super.init()
        synthesizer.delegate = self
    }

    /// Returns once it has been said, so the microphone does not hear it.
    /// `always` is for a question that needs an answer by voice.
    func say(_ words: String, always: Bool = false) async {
        guard always || VoiceSettings.speaks else { return }
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
