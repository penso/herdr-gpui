import AVFoundation
import Observation
import Speech

/// Dictation into the composer, transcribed on the phone with
/// `SpeechAnalyzer`: nothing leaves the device, and the words appear as
/// they are spoken, firming up as each phrase ends.
@MainActor
@Observable
final class Dictation {
    enum Phase: Equatable {
        case idle
        case preparing
        case listening
        case failed(String)
    }

    private(set) var phase = Phase.idle
    /// Phrases the transcriber settled on.
    private(set) var settled = ""
    /// Its guess at the phrase still being spoken.
    private(set) var guess = ""
    private var analyzer: SpeechAnalyzer?
    private var feed: AudioFeed?
    private var reader: Task<Void, Never>?

    var text: String { settled + guess }
    var active: Bool { phase == .preparing || phase == .listening }

    func start() async {
        guard !active else { return }
        phase = .preparing
        settled = ""
        guess = ""
        guard await AVAudioApplication.requestRecordPermission() else {
            phase = .failed("Allow the microphone in Settings to dictate.")
            return
        }
        guard let locale = await SpeechTranscriber.supportedLocale(equivalentTo: .current) else {
            phase = .failed("Dictation does not support your language yet.")
            return
        }
        let transcriber = SpeechTranscriber(
            locale: locale,
            transcriptionOptions: [],
            reportingOptions: [.volatileResults],
            attributeOptions: []
        )
        do {
            // The model for the language downloads once, then stays.
            if let install = try await AssetInventory.assetInstallationRequest(supporting: [transcriber]) {
                try await install.downloadAndInstall()
            }
            guard let format = await SpeechAnalyzer.bestAvailableAudioFormat(compatibleWith: [transcriber]) else {
                throw DictationError.noFormat
            }
            let session = AVAudioSession.sharedInstance()
            try session.setCategory(.record, mode: .measurement, options: .duckOthers)
            try session.setActive(true, options: .notifyOthersOnDeactivation)
            let (input, feeding) = AsyncStream.makeStream(of: AnalyzerInput.self)
            let analyzer = SpeechAnalyzer(modules: [transcriber])
            reader = Task { [weak self] in
                do {
                    for try await result in transcriber.results {
                        let words = String(result.text.characters)
                        if result.isFinal {
                            self?.settled += words
                            self?.guess = ""
                        } else {
                            self?.guess = words
                        }
                    }
                } catch {
                    self?.phase = .failed(error.localizedDescription)
                }
            }
            try await analyzer.start(inputSequence: input)
            let feed = AudioFeed(format: format, into: feeding)
            try feed.start()
            self.analyzer = analyzer
            self.feed = feed
            phase = .listening
        } catch {
            await finish()
            phase = .failed(error.localizedDescription)
        }
    }

    /// Stops listening and waits for the last phrase to settle.
    func stop() async {
        await finish()
        if active { phase = .idle }
    }

    /// Drops what was heard, as when the prompt is sent.
    func reset() {
        settled = ""
        guess = ""
        if case .failed = phase { phase = .idle }
    }

    private func finish() async {
        feed?.stop()
        feed = nil
        if let analyzer {
            try? await analyzer.finalizeAndFinishThroughEndOfInput()
        }
        analyzer = nil
        await reader?.value
        reader = nil
        settled += guess
        guess = ""
        try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
    }
}

enum DictationError: Error, LocalizedError {
    case noFormat
    case noConverter

    var errorDescription: String? {
        switch self {
        case .noFormat: "Dictation has no audio format for this language."
        case .noConverter: "The microphone's audio could not be converted for dictation."
        }
    }
}

/// The microphone, converted to the transcriber's format. Not actor
/// isolated: the tap runs on the audio thread, and its block must not
/// inherit the main actor.
private final class AudioFeed: @unchecked Sendable {
    private let engine = AVAudioEngine()
    private let format: AVAudioFormat
    private let feeding: AsyncStream<AnalyzerInput>.Continuation

    init(format: AVAudioFormat, into feeding: AsyncStream<AnalyzerInput>.Continuation) {
        self.format = format
        self.feeding = feeding
    }

    func start() throws {
        let input = engine.inputNode
        let source = input.outputFormat(forBus: 0)
        guard let converter = AVAudioConverter(from: source, to: format) else { throw DictationError.noConverter }
        let format = format
        let feeding = feeding
        input.installTap(onBus: 0, bufferSize: 4096, format: source) { buffer, _ in
            let ratio = format.sampleRate / source.sampleRate
            let capacity = AVAudioFrameCount((Double(buffer.frameLength) * ratio).rounded(.up)) + 1
            guard let converted = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: capacity) else { return }
            nonisolated(unsafe) var given = false
            var error: NSError?
            converter.convert(to: converted, error: &error) { _, status in
                if given {
                    status.pointee = .noDataNow
                    return nil
                }
                given = true
                status.pointee = .haveData
                return buffer
            }
            if error == nil, converted.frameLength > 0 {
                feeding.yield(AnalyzerInput(buffer: converted))
            }
        }
        engine.prepare()
        try engine.start()
    }

    func stop() {
        engine.inputNode.removeTap(onBus: 0)
        engine.stop()
        feeding.finish()
    }
}
