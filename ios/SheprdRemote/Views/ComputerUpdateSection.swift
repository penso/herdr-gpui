import SheprdRemoteCore
import SwiftUI

/// Sheprd on one computer: the release it runs, whether this app needs a
/// newer one, and asking it to update. Sheprd does the update itself, so
/// the phone only asks and follows.
struct ComputerUpdateSection: View {
    @Environment(AppModel.self) private var model
    let device: String
    let name: String
    @State private var update: ComputerUpdate?
    /// The computer has no update API, so this cannot ask it.
    @State private var tooOld = false
    @State private var confirming = false
    @State private var problem: String?

    private var machine: Fleet.Machine? { model.fleet.machines.first { $0.id == device } }
    private var online: Bool { if case .online = machine?.status { true } else { false } }

    var body: some View {
        Section {
            LabeledContent("Version", value: machine?.home?.version ?? "Unknown")
            if machine?.needsUpdate == true || machine?.status == .outdated {
                ComputerStatusLine(status: .outdated)
            }
            if let update, update.working {
                HStack(spacing: 8) {
                    ProgressView()
                    Text("Updating…").foregroundStyle(.secondary)
                }
            } else if let update, update.canAsk {
                Button(action(update)) { confirming = true }
            }
        } header: {
            Text("Sheprd")
        } footer: {
            if let footer { Text(footer) }
        }
        .task(id: online) { await follow() }
        .confirmationDialog("Update Sheprd on \(name)?", isPresented: $confirming, titleVisibility: .visible) {
            Button("Update and Restart") { ask() }
        } message: {
            Text("Sheprd restarts on \(name) once the update is in. Terminals and agents keep running.")
        }
    }

    private func action(_ update: ComputerUpdate) -> String {
        guard let app = update.app else { return "Update" }
        switch app.stage {
        case .available: return app.latest.map { "Update to \($0)" } ?? "Update"
        case .failed: return "Try Again"
        case .busy, .current, .disabled, .idle: return "Check for Updates"
        }
    }

    private var footer: String? {
        if let problem { return problem }
        if tooOld { return "Update Sheprd on \(name) by hand once. After that you can update it from here." }
        guard let update else { return nil }
        guard let app = update.app else { return "Sheprd is not open on \(name), so it cannot update from here." }
        switch app.stage {
        case .current: return "Sheprd is up to date."
        case .disabled, .failed: return app.detail
        case .available, .busy, .idle: return nil
        }
    }

    /// Reads the updater while this page shows, faster during an update.
    private func follow() async {
        guard online else { return }
        while !Task.isCancelled {
            await refresh()
            try? await Task.sleep(for: .seconds(update?.working == true ? 2 : 5))
        }
    }

    private func refresh() async {
        do {
            update = try await model.fleet.call(device) { client, token in
                try await client.computerUpdate(bearer: token)
            }
            tooOld = false
        } catch RemoteError.status(404, _) {
            tooOld = true
            update = nil
        } catch {
            // Restarting drops the connection; the last answer stands.
        }
    }

    private func ask() {
        problem = nil
        Task {
            do {
                update = try await model.fleet.call(device) { client, token in
                    try await client.askToUpdate(bearer: token)
                }
            } catch {
                problem = error.localizedDescription
            }
        }
    }
}
