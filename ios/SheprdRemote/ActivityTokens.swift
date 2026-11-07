import ActivityKit
import Foundation
import SheprdRemoteCore

/// Collects the push tokens ActivityKit hands the app and sends them to the
/// paired computers (`POST /api/apns`): the push-to-start token, which lets
/// a computer start an activity while the app is closed, goes to every
/// computer; each activity's own token goes only to the computer whose CA
/// fingerprint the activity carries as `computerId`, never by name.
///
/// A paired computer can put another computer's id in an activity it
/// starts. That only makes the app hand the other computer a token for an
/// activity the claimant started, which is harmless; the claimant can never
/// obtain a token for an activity another computer started.
@MainActor
@Observable
final class ActivityTokens {
    struct Running: Identifiable, Equatable {
        let id: String
        let attributes: ChatActivityFacts
        var state: ChatActivityState
        var token: String?
    }

    /// The setting that keeps this phone's activities generic: every
    /// computer leaves the chat's name and steps out of them.
    static let genericSetting = "liveActivitiesGeneric"

    private(set) var pushToStart: String?
    /// The app's own APNs token, for sealed alerts.
    private(set) var alert: String?
    private(set) var running: [String: Running] = [:]
    private(set) var lastError: String?

    /// The paired computers, kept current by the app model.
    var computers: [Computer] = []
    private let sessions: Sessions
    private var started = false

    init(sessions: Sessions) {
        self.sessions = sessions
    }

    /// Starts watching ActivityKit; safe to call more than once.
    func start() {
        guard !started else { return }
        started = true
        Task {
            for await data in Activity<ChatActivityAttributes>.pushToStartTokenUpdates {
                pushToStart = data.hexString
                await sync()
            }
        }
        Task {
            for await activity in Activity<ChatActivityAttributes>.activityUpdates {
                watch(activity)
            }
        }
        for activity in Activity<ChatActivityAttributes>.activities {
            watch(activity)
        }
    }

    /// A new APNs token from iOS: every computer gets it, with the key
    /// alerts are sealed to.
    func setAlert(_ token: String) {
        guard token != alert else { return }
        alert = token
        Task { await sync() }
    }

    private func watch(_ activity: Activity<ChatActivityAttributes>) {
        guard running[activity.id] == nil else { return }
        if activity.attributes.facts.namesChat, UserDefaults.standard.bool(forKey: Self.genericSetting) {
            Task { await activity.end(nil, dismissalPolicy: .immediate) }
            return
        }
        running[activity.id] = Running(
            id: activity.id,
            attributes: activity.attributes.facts,
            state: activity.content.state,
            token: activity.pushToken?.hexString
        )
        Task {
            for await data in activity.pushTokenUpdates {
                running[activity.id]?.token = data.hexString
                if let owner = activity.attributes.facts.fingerprint {
                    await sync(only: owner)
                }
            }
        }
        Task {
            for await content in activity.contentUpdates {
                running[activity.id]?.state = content.state
            }
        }
        Task {
            for await state in activity.activityStateUpdates where state == .ended || state == .dismissed {
                running[activity.id] = nil
                if let owner = activity.attributes.facts.fingerprint {
                    await sync(only: owner)
                }
                return
            }
        }
    }

    /// Tells the computers about Keep Generic. Turned on, it first ends
    /// every activity: a title and workspace are fixed when one starts, so
    /// no later push could take them off the lock screen, and what one last
    /// said may name the chat too. That needs no network, so it does not
    /// wait on a computer that is away; `watch` ends what such a computer
    /// starts before it hears.
    func genericChanged(to generic: Bool) async {
        if generic {
            for activity in Activity<ChatActivityAttributes>.activities {
                await activity.end(nil, dismissalPolicy: .immediate)
            }
        }
        await sync()
    }

    /// Sends each paired computer (or only the one with CA `only`) its
    /// tokens.
    func sync(only: Fingerprint? = nil) async {
        for computer in computers where only == nil || computer.fingerprint == only {
            let activities = running.values
                .filter { $0.attributes.fingerprint == computer.fingerprint }
                .compactMap { running in
                    running.token.map { API.APNsRegistration.Activity(pane: running.attributes.pane, token: $0) }
                }
            // Each computer seals to its own key. Without one, alerts would
            // arrive sealed to nobody: send neither, and the computer keeps
            // using Web Push or nothing.
            let notifyKey = alert == nil ? nil : try? NotifyKey.loadOrCreate(for: computer.fingerprint).registered
            let registration = API.APNsRegistration(
                pushToStart: pushToStart,
                activities: activities,
                alert: notifyKey == nil ? nil : alert,
                notifyKey: notifyKey,
                genericActivities: UserDefaults.standard.bool(forKey: Self.genericSetting)
            )
            do {
                let session = try await sessions.session(for: computer)
                try await session.client.registerAPNs(registration, bearer: session.token)
                lastError = nil
            } catch let RemoteError.status(503, _) {
                // A computer with no push set up. Nothing is wrong, and the
                // session is good: it answered.
                lastError = nil
            } catch {
                await sessions.forget(computer)
                lastError = "\(computer.name) (\(computer.fingerprint.hex.prefix(8))): \(error.localizedDescription)"
            }
        }
    }
}
