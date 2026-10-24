import Foundation

/// The changing part of a chat's Live Activity: `ChatActivityAttributes`'s
/// `ContentState`. The daemon writes the same keys as `content-state` in
/// its APNs payloads (`crates/herdr-remote-control/src/apns/payload.rs`), and the
/// fixtures in `Tests/HerdrRemoteCoreTests/Fixtures` hold both sides to it:
///
/// | Key | Type | |
/// | --- | --- | --- |
/// | `phase` | `"working"`, `"question"`, `"finished"`, `"failed"` | |
/// | `step` | string | current tool call or plan step |
/// | `startedAt` | number | turn start, seconds since 1970 |
/// | `cost` | string? | formatted, such as `$0.34` |
/// | `question` | string? | the question waiting |
/// | `summary` | string? | how the turn ended |
/// | `planDone`, `planTotal` | int? | plan progress |
/// | `working`, `waiting` | int, default 0 | this computer's running turns and waiting questions |
///
/// `startedAt` is decoded here as Unix seconds rather than through a
/// `JSONDecoder` date strategy, which ActivityKit does not let an app pick.
public struct ChatActivityState: Codable, Hashable, Sendable {
    public enum Phase: String, Codable, Hashable, Sendable {
        case working
        case question
        case finished
        case failed
    }

    public var phase: Phase
    public var step: String
    public var startedAt: Date
    public var cost: String?
    public var question: String?
    public var summary: String?
    public var planDone: Int?
    public var planTotal: Int?
    public var working: Int
    public var waiting: Int

    public init(
        phase: Phase,
        step: String,
        startedAt: Date,
        cost: String? = nil,
        question: String? = nil,
        summary: String? = nil,
        planDone: Int? = nil,
        planTotal: Int? = nil,
        working: Int = 0,
        waiting: Int = 0
    ) {
        self.phase = phase
        self.step = step
        self.startedAt = startedAt
        self.cost = cost
        self.question = question
        self.summary = summary
        self.planDone = planDone
        self.planTotal = planTotal
        self.working = working
        self.waiting = waiting
    }

    enum CodingKeys: String, CodingKey {
        case phase, step, startedAt, cost, question, summary, planDone, planTotal, working, waiting
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        phase = try container.decode(Phase.self, forKey: .phase)
        step = try container.decode(String.self, forKey: .step)
        let seconds = try container.decode(Double.self, forKey: .startedAt)
        startedAt = Date(timeIntervalSince1970: seconds)
        cost = try container.decodeIfPresent(String.self, forKey: .cost)
        question = try container.decodeIfPresent(String.self, forKey: .question)
        summary = try container.decodeIfPresent(String.self, forKey: .summary)
        planDone = try container.decodeIfPresent(Int.self, forKey: .planDone)
        planTotal = try container.decodeIfPresent(Int.self, forKey: .planTotal)
        working = try container.decodeIfPresent(Int.self, forKey: .working) ?? 0
        waiting = try container.decodeIfPresent(Int.self, forKey: .waiting) ?? 0
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(phase, forKey: .phase)
        try container.encode(step, forKey: .step)
        try container.encode(startedAt.timeIntervalSince1970.rounded(.down), forKey: .startedAt)
        try container.encodeIfPresent(cost, forKey: .cost)
        try container.encodeIfPresent(question, forKey: .question)
        try container.encodeIfPresent(summary, forKey: .summary)
        try container.encodeIfPresent(planDone, forKey: .planDone)
        try container.encodeIfPresent(planTotal, forKey: .planTotal)
        try container.encode(working, forKey: .working)
        try container.encode(waiting, forKey: .waiting)
    }

    /// Plan progress from 0 to 1, when the agent keeps a plan.
    public var planFraction: Double? {
        guard let done = planDone, let total = planTotal, total > 0 else { return nil }
        return min(1, max(0, Double(done) / Double(total)))
    }

    /// `Step 3 of 4` while the plan has a step in progress.
    public var planLabel: String? {
        guard let done = planDone, let total = planTotal, total > 0 else { return nil }
        return "Step \(min(done + 1, total)) of \(total)"
    }

    /// The line under the activity's title in each phase.
    public var headline: String {
        switch phase {
        case .working: step
        case .question: "Needs an answer"
        case .finished: "Finished"
        case .failed: "Stopped"
        }
    }
}

/// The fixed part of a chat's Live Activity, as plain data, so code outside
/// ActivityKit can name it. Its keys are `attributes` in a push-to-start
/// payload: `agent`, `chatTitle`, `computer`, `computerId`, `pane`,
/// `workspace`.
public struct ChatActivityFacts: Codable, Hashable, Sendable {
    /// The computer's name: display only, since any computer can pick any
    /// name.
    public var computer: String
    /// The computer's CA fingerprint as `Fingerprint.hex` writes it: 64
    /// uppercase hex digits, no separators. Links and push tokens are routed
    /// by it alone.
    public var computerId: String
    public var workspace: String
    public var chatTitle: String
    public var agent: String
    public var pane: String

    public init(
        computer: String,
        computerId: String,
        workspace: String,
        chatTitle: String,
        agent: String,
        pane: String
    ) {
        self.computer = computer
        self.computerId = computerId
        self.workspace = workspace
        self.chatTitle = chatTitle
        self.agent = agent
        self.pane = pane
    }

    /// The computer the activity says it belongs to, when `computerId` is a
    /// fingerprint at all.
    public var fingerprint: Fingerprint? {
        Fingerprint(computerId)
    }

    /// The link the activity opens.
    public var link: ChatLink {
        ChatLink(computer: computer, computerId: computerId, pane: pane)
    }

    /// `workspace · chat`, or the workspace alone for an untitled chat.
    public var place: String {
        chatTitle.isEmpty ? workspace : "\(workspace) \u{b7} \(chatTitle)"
    }
}
