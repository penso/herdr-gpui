import Foundation
import HerdrRemoteCore
import os
import WatchConnectivity

private let log = Logger(subsystem: "so.pen.herdr.remote", category: "watch")

/// The phone's end of the Watch app: it hands the Watch the chats it last
/// saw (the widget summary, with each waiting question) and sends the
/// answers and replies the Watch asks for, after checking them against
/// that summary. Delegate calls arrive on a WatchConnectivity queue.
final class WatchLink: NSObject, WCSessionDelegate, @unchecked Sendable {
    static let shared = WatchLink()

    func start() {
        guard WCSession.isSupported() else { return }
        WCSession.default.delegate = self
        WCSession.default.activate()
    }

    /// Sends the latest summary; WatchConnectivity keeps only the newest.
    func publish(_ summary: WidgetSummary) {
        let session = WCSession.default
        guard WCSession.isSupported(), session.activationState == .activated, session.isPaired, session.isWatchAppInstalled,
              let data = try? JSONEncoder().encode(summary)
        else { return }
        try? session.updateApplicationContext(["summary": data])
    }

    func session(_ session: WCSession, activationDidCompleteWith activationState: WCSessionActivationState, error: Error?) {
        if activationState == .activated { publish(WidgetSummary.load()) }
    }

    func sessionDidBecomeInactive(_ session: WCSession) {}

    func sessionDidDeactivate(_ session: WCSession) {
        // A different Watch was paired: talk to it.
        session.activate()
    }

    func session(_ session: WCSession, didReceiveMessageData messageData: Data, replyHandler: @escaping (Data) -> Void) {
        let reply = UncheckedReply(send: replyHandler)
        Task {
            let response = await Self.handle(messageData)
            reply.send((try? JSONEncoder().encode(response)) ?? Data())
        }
    }

    private static func handle(_ data: Data) async -> WatchResponse {
        // The Face ID lock means nothing reaches a chat without Face ID on
        // this phone, as Siri also refuses then. Otherwise the Watch being
        // unlocked on the wrist is the owner's check.
        guard !AppLock.isOn else {
            return WatchResponse(sent: false, message: "Herdr Remote is locked. Answer on your iPhone.")
        }
        guard let request = try? JSONDecoder().decode(WatchRequest.self, from: data) else {
            return WatchResponse(sent: false, message: "The phone could not read that.")
        }
        guard let checked = request.command(in: WidgetSummary.load()) else {
            return WatchResponse(sent: false, message: "That is no longer waiting. Open Herdr Remote on the phone.")
        }
        do {
            try await BackgroundCommands.send(checked.command, computer: checked.computer, pane: checked.pane)
            return WatchResponse(sent: true, message: nil)
        } catch {
            log.error("Could not send from the Watch: \(error.localizedDescription, privacy: .public)")
            return WatchResponse(sent: false, message: error.localizedDescription)
        }
    }
}

/// WatchConnectivity's reply handler, called once from the task that
/// answers it.
private struct UncheckedReply: @unchecked Sendable {
    let send: (Data) -> Void
}
