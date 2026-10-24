import Foundation

/// The JSON the daemon's API takes and returns (`crates/herdr-remote-control/src/web/api.rs`).
public enum API {
    /// `POST /api/pair/claim`.
    public struct ClaimRequest: Encodable, Equatable, Sendable {
        public let token: String
        public let name: String
        public let publicKey: String

        public init(token: String, name: String, publicKey: String) {
            self.token = token
            self.name = name
            self.publicKey = publicKey
        }

        enum CodingKeys: String, CodingKey {
            case token, name
            case publicKey = "public_key"
        }
    }

    public struct ClaimResponse: Decodable, Equatable, Sendable {
        public let claim: String
        /// The 6-digit matching code both screens show.
        public let code: String
    }

    /// `GET /api/pair/claim/{claim}`.
    public enum ClaimStatus: Decodable, Equatable, Sendable {
        case waiting
        case approved(Approval)
        case denied
        case expired

        public struct Approval: Decodable, Equatable, Sendable {
            public let device: String
            public let origins: [String]
            public let fingerprint: Fingerprint
            public let computer: String
        }

        enum CodingKeys: String, CodingKey { case state }

        public init(from decoder: Decoder) throws {
            let state = try decoder.container(keyedBy: CodingKeys.self).decode(String.self, forKey: .state)
            switch state {
            case "approved": self = .approved(try Approval(from: decoder))
            case "denied": self = .denied
            case "expired": self = .expired
            default: self = .waiting
            }
        }
    }

    /// `POST /api/session/challenge`.
    public struct ChallengeRequest: Encodable, Equatable, Sendable {
        public let device: String
        public init(device: String) { self.device = device }
    }

    public struct Challenge: Decodable, Equatable, Sendable {
        public let challenge: String
        public let nonce: String
    }

    /// `POST /api/session`.
    public struct SessionRequest: Encodable, Equatable, Sendable {
        public let challenge: String
        public let signature: String
        public init(challenge: String, signature: String) {
            self.challenge = challenge
            self.signature = signature
        }
    }

    public struct Session: Decodable, Equatable, Sendable {
        /// The bearer token.
        public let token: String
        /// When it stops being accepted, in milliseconds since 1970.
        public let expiresMs: UInt64

        enum CodingKeys: String, CodingKey {
            case token
            case expiresMs = "expires_ms"
        }
    }

    /// `POST /api/apns`: the app's push-to-start token and one token per
    /// Live Activity it shows for that computer, all lowercase hex. The
    /// daemon replaces its list of activities with this one.
    public struct APNsRegistration: Encodable, Equatable, Sendable {
        public struct Activity: Encodable, Equatable, Sendable {
            public let pane: String
            public let token: String
            public init(pane: String, token: String) {
                self.pane = pane
                self.token = token
            }
        }

        public let pushToStart: String?
        public let activities: [Activity]

        public init(pushToStart: String?, activities: [Activity]) {
            self.pushToStart = pushToStart
            self.activities = activities
        }

        enum CodingKeys: String, CodingKey {
            case pushToStart = "push_to_start"
            case activities
        }

        public func encode(to encoder: Encoder) throws {
            var container = encoder.container(keyedBy: CodingKeys.self)
            try container.encodeIfPresent(pushToStart, forKey: .pushToStart)
            try container.encode(activities, forKey: .activities)
        }
    }
}
