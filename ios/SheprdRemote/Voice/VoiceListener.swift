import AVFoundation
import Observation
import Speech

/// The microphone for spoken commands, transcribed on the phone. Unlike
/// `Dictation` it uses `DictationTranscriber`, the module that takes
/// contextual strings, so the transcriber is told the fleet's names: left
/// alone it hears "sheprd" as "shepherd".
@MainActor
@Observable
final class VoiceListener {
    /// How many recent levels the wave shows.
    static let bars = 14

    private(set) var levels = [Float](repeating: 0, count: VoiceListener.bars)
    private var settled = ""
    private var guess = ""
    private var analyzer: SpeechAnalyzer?
    private var feed: AudioFeed?
    private var reader: Task<Void, Never>?
    /// Which `start` is current; `stop` moves it on.
    private var attempt = 0

    var text: String { (settled + guess).trimmingCharacters(in: .whitespaces) }

    /// Listens for `names` until `stop`, calling `heard` as the words come.
    func start(names: [String], heard: @escaping @MainActor (String) -> Void) async throws {
        attempt += 1
        let mine = attempt
        settled = ""
        guess = ""
        guard await AVAudioApplication.requestRecordPermission() else { throw VoiceError.noMicrophone }
        // Commands are read as English, whatever the phone's language.
        let wanted = Locale.current.language.languageCode == .english ? Locale.current : Locale(identifier: "en-US")
        guard let locale = await DictationTranscriber.supportedLocale(equivalentTo: wanted) else { throw VoiceError.noLanguage }
        let transcriber = DictationTranscriber(locale: locale, preset: .progressiveShortDictation)
        if let install = try await AssetInventory.assetInstallationRequest(supporting: [transcriber]) {
            try await install.downloadAndInstall()
        }
        guard let format = await SpeechAnalyzer.bestAvailableAudioFormat(compatibleWith: [transcriber]) else {
            throw DictationError.noFormat
        }
        guard mine == attempt else { return }
        let session = AVAudioSession.sharedInstance()
        try session.setCategory(.record, mode: .measurement, options: .duckOthers)
        try session.setActive(true, options: .notifyOthersOnDeactivation)
        let (input, feeding) = AsyncStream.makeStream(of: AnalyzerInput.self)
        let analyzer = SpeechAnalyzer(modules: [transcriber])
        let context = AnalysisContext()
        context.contextualStrings[.general] = names
        try await analyzer.setContext(context)
        reader = Task { [weak self] in
            do {
                for try await result in transcriber.results {
                    guard let self else { return }
                    let words = String(result.text.characters)
                    if result.isFinal {
                        settled += words
                        guess = ""
                    } else {
                        guess = words
                    }
                    heard(text)
                }
            } catch {}
        }
        try await analyzer.start(inputSequence: input)
        self.analyzer = analyzer
        guard mine == attempt else {
            feeding.finish()
            await finish()
            return
        }
        let feed = AudioFeed(format: format, into: feeding) { [weak self] level in
            Task { @MainActor in self?.push(level) }
        }
        try feed.start()
        self.feed = feed
    }

    /// Stops listening and waits for the last words to settle.
    func stop() async {
        attempt += 1
        await finish()
    }

    private func push(_ level: Float) {
        levels.removeFirst()
        levels.append(level)
    }

    private func finish() async {
        feed?.stop()
        feed = nil
        if let analyzer { try? await analyzer.finalizeAndFinishThroughEndOfInput() }
        analyzer = nil
        await reader?.value
        reader = nil
        settled += guess
        guess = ""
        levels = [Float](repeating: 0, count: Self.bars)
        try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
    }
}

enum VoiceError: Error, LocalizedError {
    case noLanguage
    case noMicrophone

    var errorDescription: String? {
        switch self {
        case .noLanguage: "Voice commands are not available on this phone yet."
        case .noMicrophone: "Allow the microphone in Settings to talk to Sheprd."
        }
    }
}
