import Foundation
import HerdrRemoteCore
import Observation
import WatchConnectivity

/// The Watch's view of Herdr: the chats the phone last saw, and a way to
/// ask the phone to answer or reply. The Watch holds no keys; the phone
/// checks every request and sends it with its own.
@MainActor
@Observable
final class WatchModel: NSObject {
    private(set) var summary = WidgetSummary()
    private(set) var reachable = false

    func start() {
        #if DEBUG
        // The simulator loop: `-sample YES` shows made-up chats.
        if UserDefaults.standard.bool(forKey: "sample") { summary = .watchSample }
        #endif
        guard WCSession.isSupported() else { return }
        WCSession.default.delegate = self
        WCSession.default.activate()
    }

    /// Sends a request to the phone; answers why it did not go, if it did not.
    func send(_ request: WatchRequest) async -> String? {
        guard let data = try? JSONEncoder().encode(request) else { return "Could not send that." }
        guard WCSession.default.isReachable else { return "Your iPhone is not reachable." }
        return await withCheckedContinuation { continuation in
            let resume = Resume(continuation)
            WCSession.default.sendMessageData(data) { reply in
                let response = try? JSONDecoder().decode(WatchResponse.self, from: reply)
                resume.once(response?.sent == true ? nil : response?.message ?? "The phone did not send it.")
            } errorHandler: { error in
                resume.once(error.localizedDescription)
            }
        }
    }

    fileprivate func apply(_ context: [String: Any]) {
        guard let data = context["summary"] as? Data,
              let summary = try? JSONDecoder().decode(WidgetSummary.self, from: data)
        else { return }
        self.summary = summary
    }

    fileprivate func setReachable(_ reachable: Bool) { self.reachable = reachable }
}

extension WatchModel: WCSessionDelegate {
    nonisolated func session(_ session: WCSession, activationDidCompleteWith activationState: WCSessionActivationState, error: Error?) {
        let context = UncheckedContext(value: session.receivedApplicationContext)
        let reachable = session.isReachable
        Task { @MainActor in
            self.apply(context.value)
            self.setReachable(reachable)
        }
    }

    nonisolated func session(_ session: WCSession, didReceiveApplicationContext applicationContext: [String: Any]) {
        let context = UncheckedContext(value: applicationContext)
        Task { @MainActor in self.apply(context.value) }
    }

    nonisolated func sessionReachabilityDidChange(_ session: WCSession) {
        let reachable = session.isReachable
        Task { @MainActor in self.setReachable(reachable) }
    }
}

/// WatchConnectivity hands over plist values, which are not `Sendable`.
private struct UncheckedContext: @unchecked Sendable {
    let value: [String: Any]
}

/// Resumes a continuation once, whichever of the reply or the error comes.
private final class Resume: @unchecked Sendable {
    private var continuation: CheckedContinuation<String?, Never>?
    private let lock = NSLock()

    init(_ continuation: CheckedContinuation<String?, Never>) { self.continuation = continuation }

    func once(_ value: String?) {
        lock.lock()
        let pending = continuation
        continuation = nil
        lock.unlock()
        pending?.resume(returning: value)
    }
}

#if DEBUG
extension WidgetSummary {
    /// Made-up chats for the simulator loop.
    static let watchSample = WidgetSummary(
        chats: [
            .init(device: "d", computerId: "AB", computer: "Studio", pane: "p1", title: "Fix mise discovery", workspace: "herdr", workspaceId: "w1",
                  agent: "claude", needsYou: true, working: false,
                  question: .init(permission: 7, title: "Run git push origin fix/mise?", options: [
                      .init(id: "allow", name: "Allow once", kind: .allowOnce),
                      .init(id: "always", name: "Always allow", kind: .allowAlways),
                      .init(id: "deny", name: "Deny", kind: .rejectOnce),
                  ])),
            .init(device: "d", computerId: "AB", computer: "Studio", pane: "p2", title: "Equalize splits", workspace: "herdr", workspaceId: "w1",
                  agent: "codex", needsYou: false, working: true, question: nil),
        ],
        updated: .now
    )
}
#endif
