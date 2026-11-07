import Foundation

/// Where Sheprd's updater stands on a computer (`GET /api/v2/update`).
/// Only the app updates, so a computer without it open cannot from here.
public struct ComputerUpdate: Codable, Hashable, Sendable {
    /// The updater, or `nil` while Sheprd is not open on the computer.
    public var app: App?
    /// A request is waiting for the app to take it.
    public var asked: Bool

    public struct App: Codable, Hashable, Sendable {
        public var stage: Stage
        /// The newest release the app knows of.
        public var latest: String?
        /// Why it cannot update, or what failed.
        public var detail: String?

        public init(stage: Stage, latest: String? = nil, detail: String? = nil) {
            self.stage = stage
            self.latest = latest
            self.detail = detail
        }
    }

    public enum Stage: String, Codable, Hashable, Sendable {
        case available, busy, current, disabled, failed, idle
    }

    public init(app: App?, asked: Bool = false) {
        self.app = app
        self.asked = asked
    }

    /// Whether an update is under way there.
    public var working: Bool { asked || app?.stage == .busy }

    /// Whether the phone can ask the app to update.
    public var canAsk: Bool {
        guard let app, !working else { return false }
        return app.stage != .disabled
    }
}
