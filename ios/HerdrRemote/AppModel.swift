import ActivityKit
import Foundation
import HerdrRemoteCore
import UIKit

/// The app's state: paired computers, the tokens it keeps sending them, and
/// opening chats in the web app, where they live.
@MainActor
@Observable
final class AppModel {
    private(set) var computers: [Computer] = [] {
        didSet { tokens.computers = computers }
    }
    var message: String?
    let sessions: Sessions
    let tokens: ActivityTokens

    init() {
        let sessions = Sessions()
        self.sessions = sessions
        tokens = ActivityTokens(sessions: sessions)
        do {
            computers = try ComputerStore.load()
        } catch {
            message = error.localizedDescription
        }
        // Property observers do not run during `init`.
        tokens.computers = computers
    }

    func add(_ computer: Computer) {
        // A computer is its CA: pairing the same CA (or the same device id)
        // again replaces the old record. Names come from the computer and are
        // display only, so another computer using the same name is kept
        // beside it, never in its place.
        computers.removeAll { $0.fingerprint == computer.fingerprint || $0.device == computer.device }
        computers.append(computer)
        save()
        Task { await tokens.sync(only: computer.fingerprint) }
    }

    func forget(_ computer: Computer) {
        computers.removeAll { $0.device == computer.device }
        save()
        Task { await sessions.forget(computer) }
    }

    private func save() {
        do {
            try ComputerStore.save(computers)
        } catch {
            message = error.localizedDescription
        }
    }

    /// Opens a Live Activity's link: the chat's page at the first of the
    /// computer's addresses that answers, in Safari.
    func open(_ url: URL) async {
        guard let link = ChatLink(url: url) else { return }
        guard let fingerprint = link.fingerprint,
              let computer = computers.first(where: { $0.fingerprint == fingerprint })
        else {
            message = "\(link.computer.isEmpty ? "That computer" : link.computer) is not paired with this phone."
            return
        }
        let origin = await reachableOrigin(of: computer)
        guard let page = link.page(origin: origin) else { return }
        await UIApplication.shared.open(page)
    }

    /// Opens the computer's web app home in Safari.
    /// The name to show for `computer`: its own, plus the start of its
    /// fingerprint when another paired computer has the same name.
    func displayName(_ computer: Computer) -> String {
        let twins = computers.filter { $0.name == computer.name }.count
        guard twins > 1 else { return computer.name }
        return "\(computer.name) (\(computer.fingerprint.hex.prefix(8)))"
    }

    /// The paired computer an activity names by fingerprint, if any.
    func computer(for facts: ChatActivityFacts) -> Computer? {
        guard let fingerprint = facts.fingerprint else { return nil }
        return computers.first { $0.fingerprint == fingerprint }
    }

    func openWebApp(_ computer: Computer) async {
        let origin = await reachableOrigin(of: computer)
        guard let url = URL(string: origin + "/") else { return }
        await UIApplication.shared.open(url)
    }

    private func reachableOrigin(of computer: Computer) async -> String {
        for origin in computer.origins {
            if await RemoteClient(origin: origin, fingerprint: computer.fingerprint).ping() {
                return origin
            }
        }
        // Nothing answered; Safari shows why for the pairing address.
        return computer.origins.first ?? ""
    }

    #if DEBUG
    /// Starts a local activity with sample content, to check the layouts on
    /// a device without an APNs key.
    func startSample() {
        let facts = ChatActivityFacts(
            computer: computers.first?.name ?? "studio",
            computerId: computers.first?.fingerprint.hex ?? String(repeating: "0", count: 64),
            workspace: "herdr-gpui",
            chatTitle: "split tests",
            agent: "Codex",
            pane: "sample"
        )
        let state = ChatActivityState(
            phase: .working,
            step: "Writing mockups/index.html",
            startedAt: Date().addingTimeInterval(-250),
            cost: "$0.34",
            planDone: 2,
            planTotal: 4,
            working: 2,
            waiting: 1
        )
        do {
            _ = try Activity.request(
                attributes: ChatActivityAttributes(facts: facts),
                content: .init(state: state, staleDate: nil)
            )
        } catch {
            message = error.localizedDescription
        }
    }
    #endif
}
