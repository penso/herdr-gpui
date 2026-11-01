import SwiftUI

// New workspace, as a sheet over home B. A: one form. B: the project
// picker first, then agent and branch on later steps. C: the prompt first,
// with where and who as chips above it.

private struct AgentChoice: Identifiable {
    let id = UUID()
    var name: String
    var symbol: String
    var tint: Color
}

private let agentChoices: [AgentChoice] = [
    AgentChoice(name: "Claude", symbol: "sparkle", tint: Agent.claude.tint),
    AgentChoice(name: "Codex", symbol: "hexagon.fill", tint: Agent.codex.tint),
    AgentChoice(name: "Gemini", symbol: "diamond.fill", tint: Agent.gemini.tint),
    AgentChoice(name: "OpenCode", symbol: "chevron.left.forwardslash.chevron.right", tint: .teal),
    AgentChoice(name: "Amp", symbol: "bolt.fill", tint: .pink),
    AgentChoice(name: "Terminal", symbol: "apple.terminal", tint: Agent.terminal.tint),
]

private struct OverHome<Sheet: View>: View {
    @ViewBuilder var sheet: Sheet

    var body: some View {
        HomeTabs(selected: .home) { AttentionList() }
            .sheet(isPresented: .constant(true)) { sheet.presentationDetents([.large]) }
    }
}

struct NewWorkspaceForm: View {
    var body: some View {
        OverHome {
            NavigationStack {
                Form {
                    Section("Where") {
                        LabeledContent("Computer") {
                            HStack(spacing: 4) { Text("MacBook Pro"); Image(systemName: "chevron.up.chevron.down").font(.caption) }
                        }
                        NavigationLink {} label: {
                            LabeledContent("Project") {
                                VStack(alignment: .trailing, spacing: 0) {
                                    Text("herdr-gpui").foregroundStyle(.primary)
                                    Text("~/code").font(.caption)
                                }
                            }
                        }
                    }
                    Section {
                        Toggle("New worktree", isOn: .constant(true))
                        LabeledContent("Branch") {
                            Text("feat/share-extension").font(.body.monospaced())
                        }
                    } header: {
                        Text("Branch")
                    } footer: {
                        Text("Created from main in ~/.herdr/worktrees/herdr-gpui.")
                    }
                    Section("Agent") {
                        LazyVGrid(columns: Array(repeating: GridItem(.flexible(), spacing: 10), count: 3), spacing: 10) {
                            ForEach(agentChoices) { agent in
                                AgentTile(agent: agent, selected: agent.name == "Claude")
                            }
                        }
                        .padding(.vertical, 6)
                    }
                    Section("First prompt (optional)") {
                        Text("Add a share extension that sends a page or a screenshot to a new chat.")
                            .frame(minHeight: 70, alignment: .topLeading)
                    }
                }
                .navigationTitle("New workspace")
                .navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) { Button(role: .cancel) {} }
                    ToolbarItem(placement: .confirmationAction) {
                        Button("Create") {}.buttonStyle(.glassProminent)
                    }
                }
            }
        }
    }
}

private struct AgentTile: View {
    var agent: AgentChoice
    var selected: Bool

    var body: some View {
        VStack(spacing: 6) {
            Image(systemName: agent.symbol).font(.title3).foregroundStyle(.white)
                .frame(width: 40, height: 40)
                .background(agent.tint.gradient, in: .rect(cornerRadius: 11))
            Text(agent.name).font(.caption.weight(.medium))
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 10)
        .background(selected ? AnyShapeStyle(.tint.opacity(0.15)) : AnyShapeStyle(.fill.quinary), in: .rect(cornerRadius: 14))
        .overlay(RoundedRectangle(cornerRadius: 14).strokeBorder(selected ? AnyShapeStyle(.tint) : AnyShapeStyle(.clear), lineWidth: 2))
    }
}

struct NewWorkspacePicker: View {
    private let recent: [(String, String)] = [("herdr-gpui", "~/code"), ("herdr", "~/code")]
    private let folders: [(String, String)] = [
        ("herdr-gpui-site", "~/code"), ("herdr-acp-bridge", "~/code/experiments"),
        ("herdr-themes", "~/code/forks"), ("herdr-ios-spike", "~/Desktop"),
    ]

    var body: some View {
        OverHome {
            NavigationStack {
                List {
                    Section {
                        Picker("Computer", selection: .constant("MacBook Pro")) {
                            ForEach(["MacBook Pro", "studio", "build-01"], id: \.self) { Text($0) }
                        }
                        .pickerStyle(.segmented)
                        .listRowBackground(Color.clear)
                        .listRowInsets(EdgeInsets())
                    }
                    Section("Recent") {
                        ForEach(recent, id: \.0) { item in FolderRow(name: item.0, path: item.1, recent: true) }
                    }
                    Section("In your project folders") {
                        ForEach(folders, id: \.0) { item in FolderRow(name: item.0, path: item.1, recent: false) }
                    }
                }
                .searchable(text: .constant("herd"), placement: .navigationBarDrawer(displayMode: .always))
                .navigationTitle("Project")
                .navigationSubtitle("Step 1 of 3 · then agent, then branch")
                .navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) { Button(role: .cancel) {} }
                }
            }
        }
    }
}

private struct FolderRow: View {
    var name: String
    var path: String
    var recent: Bool

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: recent ? "clock.arrow.circlepath" : "folder").foregroundStyle(.secondary).frame(width: 24)
            VStack(alignment: .leading, spacing: 1) {
                Text(name)
                Text(path).font(.caption).foregroundStyle(.secondary)
            }
            Spacer()
            Image(systemName: "chevron.right").font(.caption.weight(.semibold)).foregroundStyle(.tertiary)
        }
    }
}

struct NewWorkspacePrompt: View {
    var body: some View {
        OverHome {
            NavigationStack {
                VStack(alignment: .leading, spacing: 16) {
                    ScrollView(.horizontal, showsIndicators: false) {
                        GlassEffectContainer {
                            HStack(spacing: 8) {
                                ChoiceChip(symbol: "laptopcomputer", text: "MacBook Pro")
                                ChoiceChip(symbol: "folder", text: "herdr-gpui")
                                ChoiceChip(symbol: "arrow.triangle.branch", text: "feat/share-extension")
                            }
                        }
                    }
                    HStack(spacing: 10) {
                        AgentIcon(agent: .claude, size: 30)
                        VStack(alignment: .leading, spacing: 0) {
                            Text("Claude").font(.subheadline.weight(.semibold))
                            Text("Opus 5.5 · High").font(.caption).foregroundStyle(.secondary)
                        }
                        Image(systemName: "chevron.up.chevron.down").font(.caption).foregroundStyle(.secondary)
                    }
                    Text("Add a share extension that sends a page or a screenshot to a new chat, and pick the workspace in the sheet.")
                        .font(.title3)
                    Spacer()
                    Text("Recent prompts").font(.footnote.weight(.semibold)).foregroundStyle(.secondary)
                    ForEach(["Review the open pull request", "Fix the failing CI job on main"], id: \.self) { prompt in
                        Label(prompt, systemImage: "arrow.counterclockwise").font(.subheadline)
                    }
                    HStack {
                        Button {} label: { Image(systemName: "photo") }.buttonStyle(.glass)
                        Button {} label: { Image(systemName: "mic.fill") }.buttonStyle(.glass)
                        Spacer()
                        Button {} label: { Label("Start", systemImage: "arrow.up").font(.headline) }
                            .buttonStyle(.glassProminent).controlSize(.large)
                    }
                }
                .padding(20)
                .navigationTitle("New workspace")
                .navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) { Button(role: .cancel) {} }
                }
            }
        }
    }
}

private struct ChoiceChip: View {
    var symbol: String
    var text: String

    var body: some View {
        HStack(spacing: 6) {
            Image(systemName: symbol)
            Text(text).lineLimit(1)
            Image(systemName: "chevron.down").font(.caption2.weight(.bold)).foregroundStyle(.secondary)
        }
        .font(.subheadline.weight(.medium))
        .padding(.horizontal, 12).padding(.vertical, 8)
        .glassEffect(.regular.interactive(), in: .capsule)
    }
}
