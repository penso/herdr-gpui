import SwiftUI

// Chat. A: the picked focus layout (2C) made native. B: a document with
// the running step pinned under the header. C: conversation bubbles with
// a plan rail and slash completion open.

private struct ChatHeader: ToolbarContent {
    var body: some ToolbarContent {
        ToolbarItem(placement: .principal) {
            VStack(spacing: 0) {
                HStack(spacing: 4) {
                    Text("Equalize splits command").font(.headline)
                    Image(systemName: "chevron.down").font(.caption2.weight(.bold)).foregroundStyle(.secondary)
                }
                Text("Opus 5.5 · High · Bypass permissions").font(.caption).foregroundStyle(.secondary)
            }
        }
        ToolbarItem(placement: .topBarTrailing) {
            ContextRing(fraction: 0.42)
        }
    }
}

struct ChatFocus: View {
    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 18) {
                    VStack(spacing: 8) {
                        FoldedTurn(index: 1, summary: "Find where split ratios live", meta: "2m 14s · $0.42")
                        FoldedTurn(index: 2, summary: "Add a command for it", meta: "48s · $0.18")
                    }
                    Text("Make it even out nested splits too, and add tests.")
                        .padding(14)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .background(.tint.opacity(0.12), in: .rect(cornerRadius: 18))
                    VStack(alignment: .leading, spacing: 0) {
                        ForEach(Array(Step.sample.enumerated()), id: \.element.id) { index, step in
                            HStack(alignment: .top, spacing: 12) {
                                VStack(spacing: 0) {
                                    StepIcon(step: step)
                                    if index < Step.sample.count - 1 {
                                        Rectangle().fill(.quaternary).frame(width: 2).frame(maxHeight: .infinity)
                                    }
                                }
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(step.title).font(.subheadline.weight(.medium))
                                        .foregroundStyle(step.state == .pending ? .tertiary : .primary)
                                    Text(step.detail).font(.caption).foregroundStyle(.secondary)
                                }
                                .padding(.bottom, 14)
                            }
                        }
                    }
                    Text(.init(SampleAnswer.intro))
                    CodeBlock(language: "rust", code: SampleAnswer.code)
                    Text(.init(SampleAnswer.outro))
                }
                .padding(.horizontal, 16)
                .padding(.bottom, 80)
            }
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { ChatHeader() }
            .safeAreaInset(edge: .bottom) { Composer() }
        }
    }
}

private struct FoldedTurn: View {
    var index: Int
    var summary: String
    var meta: String

    var body: some View {
        HStack {
            Text("\(index)").font(.caption.bold()).foregroundStyle(.secondary)
                .frame(width: 22, height: 22).background(.quaternary, in: .circle)
            Text(summary).font(.subheadline).lineLimit(1)
            Spacer()
            Text(meta).font(.caption.monospacedDigit()).foregroundStyle(.secondary)
            Image(systemName: "chevron.right").font(.caption.weight(.semibold)).foregroundStyle(.tertiary)
        }
        .padding(.horizontal, 12).padding(.vertical, 10)
        .background(.fill.quinary, in: .rect(cornerRadius: 14))
    }
}

struct ChatDocument: View {
    var body: some View {
        NavigationStack { ChatDocumentContent() }
    }
}

/// Chat B without its navigation stack, for the iPad split view and sheets.
struct ChatDocumentContent: View {
    var body: some View {
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    HStack(alignment: .top, spacing: 10) {
                        Rectangle().fill(.tint).frame(width: 3)
                        Text("Make it even out nested splits too, and add tests.")
                            .font(.title3.weight(.semibold))
                    }
                    .fixedSize(horizontal: false, vertical: true)
                    DisclosureGroup {
                        VStack(alignment: .leading, spacing: 8) {
                            ForEach(Step.sample) { step in
                                Label(step.title, systemImage: step.symbol).font(.subheadline)
                            }
                        }
                        .padding(.top, 8)
                    } label: {
                        HStack {
                            Image(systemName: "list.bullet.indent")
                            Text("5 steps · 3 files changed · +140 −3").font(.subheadline)
                        }
                        .foregroundStyle(.secondary)
                    }
                    Divider()
                    Text(.init(SampleAnswer.intro)).font(.body).lineSpacing(3)
                    CodeBlock(language: "rust", code: SampleAnswer.code)
                    DiffBlock()
                    Text(.init(SampleAnswer.outro))
                    Text("Earlier in this chat").font(.footnote.weight(.semibold)).foregroundStyle(.secondary)
                        .padding(.top, 12)
                    Text("2 turns · 3m 02s · $0.60").font(.footnote).foregroundStyle(.tertiary)
                }
                .padding(.horizontal, 20)
                .padding(.top, 52)
                .padding(.bottom, 80)
            }
            .overlay(alignment: .top) {
                HStack(spacing: 8) {
                    ProgressView().controlSize(.small)
                    Text("Running just ci").font(.subheadline.weight(.medium))
                    Text("clippy").font(.caption.monospaced()).foregroundStyle(.secondary)
                    Spacer()
                    Text("0:42").font(.subheadline.monospacedDigit()).foregroundStyle(.secondary)
                }
                .padding(.horizontal, 14).padding(.vertical, 9)
                .glassEffect(.regular, in: .capsule)
                .padding(.horizontal, 16)
            }
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { ChatHeader() }
            .safeAreaInset(edge: .bottom) { Composer() }
    }
}

private struct DiffBlock: View {
    private let lines: [(String, String)] = [
        (" ", "fn equalized(&self) -> Ratios {"),
        ("-", "    self.ratios.clone()"),
        ("+", "    let share = 1.0 / self.len() as f32;"),
        ("+", "    self.map(|node| node.equalized(share))"),
        (" ", "}"),
    ]

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text("crates/herdr-commands/src/splits.rs").font(.caption.monospaced()).foregroundStyle(.secondary)
                .padding(.horizontal, 12).padding(.vertical, 8)
            ForEach(Array(lines.enumerated()), id: \.offset) { _, line in
                Text("\(line.0) \(line.1)").font(.system(.caption, design: .monospaced))
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 12).padding(.vertical, 1)
                    .background(line.0 == "+" ? Color.green.opacity(0.18) : line.0 == "-" ? Color.red.opacity(0.18) : .clear)
            }
            Spacer().frame(height: 8)
        }
        .background(.fill.quinary, in: .rect(cornerRadius: 12))
    }
}

struct ChatBubbles: View {
    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 12) {
                    PlanRail()
                    HStack {
                        Spacer(minLength: 60)
                        Text("Make it even out nested splits too, and add tests.")
                            .padding(.horizontal, 14).padding(.vertical, 10)
                            .foregroundStyle(.white)
                            .background(.tint, in: .rect(cornerRadius: 20))
                    }
                    FlowChips()
                    VStack(alignment: .leading, spacing: 10) {
                        Text(.init(SampleAnswer.intro))
                        CodeBlock(language: "rust", code: SampleAnswer.code)
                    }
                    .padding(14)
                    .background(.fill.quinary, in: .rect(cornerRadius: 20))
                    HStack(spacing: 14) {
                        Label("6m 12s", systemImage: "clock")
                        Label("$0.84", systemImage: "dollarsign.circle")
                        Label("62 tok/s", systemImage: "speedometer")
                    }
                    .font(.caption).foregroundStyle(.secondary)
                }
                .padding(.horizontal, 16)
                .padding(.bottom, 220)
            }
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { ChatHeader() }
            .safeAreaInset(edge: .bottom) {
                VStack(spacing: 8) {
                    SlashMenu()
                    Composer(placeholder: "/rev", working: false)
                }
            }
        }
    }
}

private struct PlanRail: View {
    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Text("Plan").font(.caption.weight(.semibold))
                Spacer()
                Text("3 of 5").font(.caption.monospacedDigit()).foregroundStyle(.secondary)
            }
            HStack(spacing: 4) {
                ForEach(Step.sample) { step in
                    Capsule().fill(step.state == .done ? AnyShapeStyle(.tint) : step.state == .running ? AnyShapeStyle(.tint.opacity(0.5)) : AnyShapeStyle(.quaternary))
                        .frame(height: 5)
                }
            }
            Text("Now: Run just ci").font(.caption).foregroundStyle(.secondary)
        }
        .padding(12)
        .glassEffect(.regular, in: .rect(cornerRadius: 16))
    }
}

private struct FlowChips: View {
    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 6) {
                ForEach(Step.sample.prefix(4)) { step in
                    Label(step.title, systemImage: step.symbol)
                        .font(.caption)
                        .padding(.horizontal, 10).padding(.vertical, 6)
                        .background(.fill.tertiary, in: .capsule)
                }
            }
        }
    }
}

private struct SlashMenu: View {
    private let commands: [(String, String)] = [
        ("/review", "Review the current branch"),
        ("/review-pr", "Review a pull request by number"),
        ("/revert", "Undo the last turn's edits"),
    ]

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(commands, id: \.0) { command in
                HStack {
                    Text(command.0).font(.body.monospaced().weight(.medium))
                    Text(command.1).font(.subheadline).foregroundStyle(.secondary).lineLimit(1)
                    Spacer()
                }
                .padding(.horizontal, 14).padding(.vertical, 10)
            }
        }
        .glassEffect(.regular, in: .rect(cornerRadius: 20))
        .padding(.horizontal, 16)
    }
}
