import ActivityKit
import AppIntents
import Foundation
import HerdrRemoteCore
import Security

struct SpikeActivityAttributes: ActivityAttributes {
    struct ContentState: Codable, Hashable {
        var question: String
        /// What the last button press got back, shown on the lock screen.
        var result: String?
    }

    var chat: String
}

/// The address and CA the activity's buttons call, saved by the app in the
/// App Group so the intent finds them in whichever process runs it.
enum SpikeTarget {
    static var link: String? {
        get { UserDefaults(suiteName: SpikeIDs.appGroup)?.string(forKey: "pairing-link") }
        set { UserDefaults(suiteName: SpikeIDs.appGroup)?.set(newValue, forKey: "pairing-link") }
    }
}

/// A Live Activity button: calls the computer over pinned TLS from the
/// lock screen without opening the app, then shows what it got.
struct SpikeAnswerIntent: LiveActivityIntent {
    static let title: LocalizedStringResource = "Answer"

    @Parameter(title: "Option") var option: String

    init() {}

    init(option: String) {
        self.option = option
    }

    func perform() async throws -> some IntentResult {
        let result = await Self.call()
        for activity in Activity<SpikeActivityAttributes>.activities {
            var state = activity.content.state
            state.result = "\(option): \(result)"
            await activity.update(.init(state: state, staleDate: nil))
        }
        return .result()
    }

    /// GET /api/ping at the saved pairing link's address, trusting only its CA.
    static func call() async -> String {
        guard let text = SpikeTarget.link, let link = try? PairingLink(text),
              let url = URL(string: link.origin + "/api/ping")
        else { return "no pairing link saved" }
        let started = Date()
        let session = URLSession(
            configuration: .ephemeral,
            delegate: SpikePinned(fingerprint: link.fingerprint),
            delegateQueue: nil
        )
        defer { session.finishTasksAndInvalidate() }
        do {
            let (_, response) = try await session.data(from: url)
            let status = (response as? HTTPURLResponse)?.statusCode ?? 0
            return "HTTP \(status) in \(Int(Date().timeIntervalSince(started) * 1000)) ms"
        } catch {
            return "failed: \(error.localizedDescription)"
        }
    }
}

/// The same pinning as the app's `PinnedTrust`, for the spike's intent.
final class SpikePinned: NSObject, URLSessionDelegate, Sendable {
    let fingerprint: Fingerprint

    init(fingerprint: Fingerprint) {
        self.fingerprint = fingerprint
    }

    func urlSession(
        _ session: URLSession,
        didReceive challenge: URLAuthenticationChallenge
    ) async -> (URLSession.AuthChallengeDisposition, URLCredential?) {
        guard let trust = challenge.protectionSpace.serverTrust else { return (.performDefaultHandling, nil) }
        let chain = (SecTrustCopyCertificateChain(trust) as? [SecCertificate]) ?? []
        guard let index = fingerprint.match(in: chain.map { SecCertificateCopyData($0) as Data }),
              SecTrustSetAnchorCertificates(trust, [chain[index]] as CFArray) == errSecSuccess,
              SecTrustSetAnchorCertificatesOnly(trust, true) == errSecSuccess,
              SecTrustEvaluateWithError(trust, nil)
        else { return (.cancelAuthenticationChallenge, nil) }
        return (.useCredential, URLCredential(trust: trust))
    }
}
