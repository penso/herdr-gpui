import ActivityKit
import HerdrRemoteCore
import SwiftUI

/// Paired computers and the activities running now, with pairing a new
/// computer and opening the web app.
struct ContentView: View {
    @Environment(AppModel.self) private var model
    @State private var pairing = false

    var body: some View {
        NavigationStack {
            List {
                if !ActivityAuthorizationInfo().areActivitiesEnabled {
                    Section {
                        Label(
                            "Live Activities are off for Herdr Remote. Turn them on in Settings.",
                            systemImage: "exclamationmark.triangle"
                        )
                    }
                }
                computers
                activities
                if let error = model.tokens.lastError ?? model.message {
                    Section("Last problem") {
                        Text(error).font(.footnote).foregroundStyle(.secondary)
                    }
                }
                #if DEBUG
                Section("Preview") {
                    Button("Start a sample activity") { model.startSample() }
                }
                #endif
            }
            .navigationTitle("Herdr Remote")
            .toolbar {
                Button {
                    pairing = true
                } label: {
                    Label("Pair a computer", systemImage: "plus")
                }
            }
            .sheet(isPresented: $pairing) {
                PairView()
            }
            .overlay {
                if model.computers.isEmpty {
                    ContentUnavailableView {
                        Label("No computers yet", systemImage: "desktopcomputer")
                    } description: {
                        Text("In Herdr, open Remote control > Pair device, then scan its QR here.")
                    } actions: {
                        Button("Pair a computer") { pairing = true }
                            .buttonStyle(.borderedProminent)
                    }
                }
            }
        }
    }

    @ViewBuilder
    private var computers: some View {
        if !model.computers.isEmpty {
            Section("Computers") {
                ForEach(model.computers) { computer in
                    VStack(alignment: .leading, spacing: 6) {
                        Text(model.displayName(computer)).font(.headline)
                        Text(computer.origins.first ?? "")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                        Button("Open in Herdr") {
                            Task { await model.openWebApp(computer) }
                        }
                        .buttonStyle(.bordered)
                    }
                    .swipeActions {
                        Button("Forget", role: .destructive) { model.forget(computer) }
                    }
                }
            }
        }
    }

    @ViewBuilder
    private var activities: some View {
        let running = model.tokens.running.values.sorted { $0.state.startedAt > $1.state.startedAt }
        if !running.isEmpty {
            Section("Running now") {
                ForEach(running) { activity in
                    Button {
                        Task { await model.open(activity.attributes.link.url) }
                    } label: {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(activity.attributes.place).font(.subheadline.weight(.semibold))
                            Text(subtitle(activity))
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                    }
                }
            }
        }
    }

    /// The paired computer it belongs to (by fingerprint), the agent, and
    /// what it is doing.
    private func subtitle(_ activity: ActivityTokens.Running) -> String {
        let owner = model.computer(for: activity.attributes).map(model.displayName)
        return [owner, activity.attributes.agent, activity.state.headline]
            .compactMap { $0 }
            .joined(separator: " \u{b7} ")
    }
}
