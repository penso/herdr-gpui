import SwiftUI

// One workspace, pushed from the Spaces list: its tabs, and a way to open
// a new agent or terminal. A: a grouped list with a glass toolbar. B: tab
// cards with the last lines of each, New tab as the last card. C: one-tap
// launchers along the bottom. D: the New tab sheet open over the list.

/// Branch, pull request, and where it lives, above the tabs.
private struct SpaceSummary: View {
    var space = SpaceSample.detail

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Label(space.branch, systemImage: "arrow.triangle.branch")
                .font(.subheadline.monospaced()).lineLimit(1).truncationMode(.middle)
            if let pr = space.pr {
                HStack(spacing: 6) {
                    Image(systemName: "arrow.triangle.pull").foregroundStyle(pr.tone.color)
                    Text(verbatim: "#\(pr.number) Mobile app preview").lineLimit(1)
                    Spacer()
                    Text("Review").font(.caption.weight(.semibold)).foregroundStyle(pr.tone.color)
                        .padding(.horizontal, 7).padding(.vertical, 2)
                        .background(pr.tone.color.opacity(0.15), in: .capsule)
                }
                .font(.subheadline)
            }
            Label("m4max · herdr-gpui", systemImage: "laptopcomputer").font(.caption).foregroundStyle(.secondary)
        }
    }
}

private struct TabRow: View {
    var tab: SpaceTab
    var showsPreview = false

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            AgentIcon(agent: tab.agent, size: 32)
            VStack(alignment: .leading, spacing: 2) {
                HStack {
                    Text(tab.title).lineLimit(1)
                    Spacer(minLength: 6)
                    if let elapsed = tab.elapsed {
                        Text(elapsed).font(.footnote.monospacedDigit()).foregroundStyle(.secondary)
                    }
                    StatusMark(status: tab.status)
                }
                Text(tab.detail).font(.subheadline).foregroundStyle(tab.status == .needsYou ? .orange : .secondary)
                    .lineLimit(1)
                if showsPreview, let last = tab.preview.last {
                    Text(last).font(.caption.monospaced()).foregroundStyle(.tertiary).lineLimit(1)
                }
            }
        }
        .padding(.vertical, 2)
    }
}

// MARK: A

struct SpaceDetailList: View {
    var body: some View {
        List {
            Section { SpaceSummary() }
            Section("Agents") {
                ForEach(SpaceSample.tabs.filter { $0.agent != .terminal }) { TabRow(tab: $0, showsPreview: true) }
            }
            Section("Terminals") {
                ForEach(SpaceSample.tabs.filter { $0.agent == .terminal }) { TabRow(tab: $0, showsPreview: true) }
            }
        }
        .navigationTitle(SpaceSample.detail.name)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) { Button {} label: { Image(systemName: "ellipsis") } }
            ToolbarItem(placement: .bottomBar) {
                Button {} label: { Label("New agent", systemImage: "sparkle").labelStyle(.titleAndIcon).padding(.horizontal, 6) }
            }
            ToolbarSpacer(.flexible, placement: .bottomBar)
            ToolbarItem(placement: .bottomBar) {
                Button {} label: { Label("New terminal", systemImage: "apple.terminal").labelStyle(.titleAndIcon).padding(.horizontal, 6) }
            }
        }
        .toolbar(.hidden, for: .tabBar)
    }
}

// MARK: B

struct SpaceDetailCards: View {
    private let columns = [GridItem(.flexible(), spacing: 12), GridItem(.flexible(), spacing: 12)]

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                SpaceSummary()
                    .padding(14)
                    .background(.fill.quaternary, in: .rect(cornerRadius: 18))
                Text("Tabs").font(.headline)
                LazyVGrid(columns: columns, spacing: 12) {
                    ForEach(SpaceSample.tabs) { card($0) }
                    newCard
                }
            }
            .padding(16)
        }
        .navigationTitle(SpaceSample.detail.name)
        .navigationBarTitleDisplayMode(.inline)
    }

    private func card(_ tab: SpaceTab) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 6) {
                AgentIcon(agent: tab.agent, size: 22)
                Text(tab.title).font(.subheadline.weight(.semibold)).lineLimit(1)
                Spacer(minLength: 0)
                StatusMark(status: tab.status)
            }
            VStack(alignment: .leading, spacing: 2) {
                ForEach(tab.preview, id: \.self) { line in
                    Text(line).font(.system(size: 10, design: .monospaced)).lineLimit(1)
                }
            }
            .foregroundStyle(.green.opacity(0.9))
            .frame(maxWidth: .infinity, minHeight: 56, alignment: .topLeading)
            .padding(8)
            .background(.black.opacity(0.85), in: .rect(cornerRadius: 8))
            Text(tab.detail).font(.caption).foregroundStyle(tab.status == .needsYou ? .orange : .secondary).lineLimit(1)
        }
        .padding(10)
        .background(.fill.quaternary, in: .rect(cornerRadius: 16))
        .overlay(RoundedRectangle(cornerRadius: 16).strokeBorder(tab.status == .needsYou ? .orange : .clear, lineWidth: 1.5))
    }

    private var newCard: some View {
        VStack(spacing: 8) {
            Image(systemName: "plus").font(.title2.weight(.medium))
            Text("New tab").font(.subheadline.weight(.semibold))
            Text("Agent or terminal").font(.caption).foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity, minHeight: 140)
        .foregroundStyle(.tint)
        .background(RoundedRectangle(cornerRadius: 16).strokeBorder(.tint, style: StrokeStyle(lineWidth: 1.5, dash: [5, 4])))
    }
}

// MARK: C

struct SpaceDetailLauncher: View {
    var body: some View {
        List {
            Section { SpaceSummary() }
            Section {
                ForEach(SpaceSample.tabs) { TabRow(tab: $0) }
            } header: {
                Text("4 tabs")
            }
        }
        .navigationTitle(SpaceSample.detail.name)
        .navigationBarTitleDisplayMode(.inline)
        .safeAreaInset(edge: .bottom) {
            VStack(alignment: .leading, spacing: 8) {
                Text("New tab").font(.caption.weight(.semibold)).foregroundStyle(.secondary).padding(.leading, 6)
                HStack(spacing: 10) {
                    ForEach([Agent.claude, .codex, .gemini, .terminal], id: \.name) { agent in
                        Button {} label: {
                            VStack(spacing: 4) {
                                AgentIcon(agent: agent, size: 30)
                                Text(agent.name).font(.caption2.weight(.medium))
                            }
                            .frame(maxWidth: .infinity)
                        }
                        .buttonStyle(.plain)
                    }
                }
            }
            .padding(12)
            .glassEffect(.regular, in: .rect(cornerRadius: 26))
            .padding(.horizontal, 16)
        }
        .toolbar(.hidden, for: .tabBar)
    }
}

// MARK: D

struct SpaceDetailSheet: View {
    var body: some View {
        List {
            Section { SpaceSummary() }
            Section("Needs you") { TabRow(tab: SpaceSample.tabs[0]) }
            Section("Tabs") { ForEach(SpaceSample.tabs.dropFirst()) { TabRow(tab: $0) } }
        }
        .navigationTitle(SpaceSample.detail.name)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) { Button {} label: { Image(systemName: "plus") } }
        }
        .sheet(isPresented: .constant(true)) { NewTabSheet().presentationDetents([.medium]) }
    }
}

private struct NewTabSheet: View {
    var body: some View {
        NavigationStack {
            List {
                Section {
                    HStack {
                        Text("What should it do?").foregroundStyle(.tertiary)
                        Spacer()
                        Image(systemName: "mic.fill").foregroundStyle(.secondary)
                    }
                    .frame(minHeight: 60, alignment: .top)
                }
                Section {
                    ScrollView(.horizontal, showsIndicators: false) {
                        HStack(spacing: 10) {
                            ForEach([Agent.claude, .codex, .gemini, .terminal], id: \.name) { agent in
                                HStack(spacing: 6) {
                                    AgentIcon(agent: agent, size: 22)
                                    Text(agent.name).font(.subheadline.weight(.medium))
                                }
                                .padding(.horizontal, 10).padding(.vertical, 7)
                                .background(agent == .claude ? AnyShapeStyle(.tint.opacity(0.2)) : AnyShapeStyle(.fill.tertiary), in: .capsule)
                            }
                        }
                    }
                }
            }
            .navigationTitle("New tab")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button {} label: { Image(systemName: "xmark") } }
                ToolbarItem(placement: .confirmationAction) {
                    Button {} label: { Image(systemName: "arrow.up") }.buttonStyle(.glassProminent)
                }
            }
        }
    }
}
