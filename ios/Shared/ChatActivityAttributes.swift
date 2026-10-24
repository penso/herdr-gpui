import ActivityKit
import Foundation
import HerdrRemoteCore

/// A chat's running turn as a Live Activity. Compiled into both the app and
/// the widget extension; the daemon starts it with push-to-start, naming
/// this type as `attributes-type`.
///
/// Wire shape (see `ChatActivityState` for `content-state`):
///
/// ```json
/// "attributes": { "agent": "Claude", "chatTitle": "remote plan",
///                 "computer": "studio", "computerId": "AB12…(64 hex)",
///                 "pane": "w1-p3", "workspace": "herdr-gpui" }
/// ```
///
/// The fields are decoded flat through `ChatActivityFacts`, so the keys are
/// declared once, in `HerdrRemoteCore`, where `swift test` checks them
/// against the daemon's fixtures.
struct ChatActivityAttributes: ActivityAttributes {
    typealias ContentState = ChatActivityState

    var facts: ChatActivityFacts

    var computer: String { facts.computer }
    /// The computer's CA fingerprint, uppercase hex without separators: what
    /// links and push tokens are routed by.
    var computerId: String { facts.computerId }
    var workspace: String { facts.workspace }
    var chatTitle: String { facts.chatTitle }
    var agent: String { facts.agent }
    var pane: String { facts.pane }

    init(facts: ChatActivityFacts) {
        self.facts = facts
    }

    init(from decoder: Decoder) throws {
        facts = try ChatActivityFacts(from: decoder)
    }

    func encode(to encoder: Encoder) throws {
        try facts.encode(to: encoder)
    }

    /// The link the activity opens: the app turns it into the chat's page.
    var link: ChatLink {
        facts.link
    }
}
