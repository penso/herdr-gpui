import SwiftUI

// What you see while talking to the app. Four layouts, each in four states:
// 0 mid-sentence, 1 the plan waiting for a yes, 2 one question back,
// 3 done. All show the same sentence, so only the layout differs.

/// One stretch of what was said; `slot` marks a part the app understood.
struct Heard: Identifiable {
    let id = UUID()
    var text: String
    var slot: VoiceSlot? = nil
}

enum VoiceSlot {
    case computer, topic, agent, task

    var tint: Color {
        switch self {
        case .computer: .blue
        case .topic: .purple
        case .agent: Color(red: 0.85, green: 0.47, blue: 0.34)
        case .task: .green
        }
    }
}

enum VoiceSample {
    static let partial: [Heard] = [
        Heard(text: "Create a worktree on "),
        Heard(text: "studio", slot: .computer),
        Heard(text: " about "),
        Heard(text: "the login redirect bug", slot: .topic),
        Heard(text: " and spin up"),
    ]
    static let whole: [Heard] = [
        Heard(text: "Create a worktree on "),
        Heard(text: "studio", slot: .computer),
        Heard(text: " about "),
        Heard(text: "the login redirect bug", slot: .topic),
        Heard(text: " and spin up "),
        Heard(text: "Claude", slot: .agent),
        Heard(text: " asking it to "),
        Heard(text: "fix the redirect loop after OAuth", slot: .task),
    ]
    static let task = "Fix the redirect loop after OAuth"
    static let branch = "fix/login-redirect"
    static let projects = ["sheprd", "site", "moltis"]

    static func text(_ parts: [Heard], guess: Bool = true) -> Text {
        var whole = AttributedString()
        for part in parts {
            var piece = AttributedString(part.text)
            if let slot = part.slot {
                piece.foregroundColor = slot.tint
                piece.font = .body.weight(.semibold)
            }
            whole.append(piece)
        }
        return Text(whole)
    }
}

/// The microphone's level. Fixed heights, so screenshots are stable.
struct VoiceWave: View {
    var tint: Color = .accentColor
    var height: CGFloat = 28
    private let levels: [CGFloat] = [0.2, 0.45, 0.8, 0.55, 1, 0.7, 0.35, 0.6, 0.9, 0.5, 0.25, 0.65, 0.4, 0.2]

    var body: some View {
        HStack(spacing: 3) {
            ForEach(levels.indices, id: \.self) { index in
                Capsule().fill(tint.gradient)
                    .frame(width: 3, height: max(4, height * levels[index]))
            }
        }
        .frame(height: height)
    }
}

/// One line of the plan. An empty one says what is still missing.
struct VoiceRow: View {
    var symbol: String
    var label: String
    var value: String?
    var detail: String? = nil
    var tint: Color = .secondary
    var asking = false

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: symbol)
                .font(.body.weight(.medium))
                .foregroundStyle(value == nil ? AnyShapeStyle(.tertiary) : AnyShapeStyle(tint))
                .frame(width: 26)
            VStack(alignment: .leading, spacing: 1) {
                Text(label).font(.caption).foregroundStyle(.secondary)
                if let value {
                    Text(value).font(.body.weight(.medium)).lineLimit(2)
                } else {
                    Text(asking ? "Which one?" : "Listening…")
                        .font(.body).foregroundStyle(asking ? AnyShapeStyle(.orange) : AnyShapeStyle(.tertiary))
                }
            }
            Spacer(minLength: 8)
            if let detail { Text(detail).font(.caption).foregroundStyle(.secondary) }
            if value != nil {
                Image(systemName: "chevron.up.chevron.down").font(.caption2).foregroundStyle(.tertiary)
            }
        }
        .padding(.vertical, 7)
    }
}

/// The plan for `state`, shared by the layouts that show rows.
struct VoicePlanRows: View {
    var state: Int

    var body: some View {
        VStack(spacing: 0) {
            VoiceRow(symbol: "desktopcomputer", label: "Computer", value: "studio", detail: "6 cores spare", tint: VoiceSlot.computer.tint)
            Divider().padding(.leading, 38)
            VoiceRow(symbol: "folder", label: "Project", value: state == 2 ? nil : "sheprd",
                     detail: state == 2 ? nil : "last used here", asking: state == 2)
            Divider().padding(.leading, 38)
            VoiceRow(symbol: "arrow.triangle.branch", label: "New worktree", value: VoiceSample.branch, tint: VoiceSlot.topic.tint)
            Divider().padding(.leading, 38)
            VoiceRow(symbol: "sparkle", label: "Agent", value: state == 0 ? nil : "Claude", tint: VoiceSlot.agent.tint)
            Divider().padding(.leading, 38)
            VoiceRow(symbol: "text.bubble", label: "First prompt", value: state == 0 ? nil : VoiceSample.task, tint: VoiceSlot.task.tint)
        }
    }
}

struct VoiceConfirm: View {
    var body: some View {
        HStack(spacing: 10) {
            Button {} label: { Text("Cancel").frame(maxWidth: .infinity).frame(height: 44) }
                .buttonStyle(.glass)
            Button {} label: {
                Text("Start").frame(maxWidth: .infinity).frame(height: 44)
            }
            .buttonStyle(.glassProminent)
        }
    }
}

struct VoiceProjectChoices: View {
    var body: some View {
        HStack(spacing: 8) {
            ForEach(VoiceSample.projects, id: \.self) { project in
                Button(project) {}.buttonStyle(.glass)
            }
        }
    }
}

struct VoiceDone: View {
    var body: some View {
        VStack(spacing: 14) {
            Image(systemName: "checkmark.circle.fill").font(.system(size: 44)).foregroundStyle(.green)
            Text("Claude is on it").font(.title3.weight(.semibold))
            Text("sheprd on studio, branch \(VoiceSample.branch)")
                .font(.subheadline).foregroundStyle(.secondary).multilineTextAlignment(.center)
            HStack(spacing: 10) {
                Button {} label: { Text("Done").frame(maxWidth: .infinity).frame(height: 44) }.buttonStyle(.glass)
                Button {} label: { Text("Open chat").frame(maxWidth: .infinity).frame(height: 44) }.buttonStyle(.glassProminent)
            }
        }
    }
}

// MARK: A: a sheet with the sentence above the plan

/// The sentence and the plan together in a sheet over the app. Rows fill
/// in as the words land, and the sentence colors what was understood.
struct VoiceSheetLayout: View {
    var state: Int

    var body: some View {
        ZStack(alignment: .bottom) {
            HomeAttention().blur(radius: 1.5)
            Color.black.opacity(0.3).ignoresSafeArea()
            VStack(alignment: .leading, spacing: 14) {
                Capsule().fill(.tertiary).frame(width: 36, height: 5).frame(maxWidth: .infinity)
                if state == 3 {
                    VoiceDone()
                } else {
                    VoiceSample.text(state == 0 ? VoiceSample.partial : VoiceSample.whole)
                        .font(.body).foregroundStyle(.secondary)
                    VoicePlanRows(state: state)
                        .padding(.horizontal, 14).padding(.vertical, 6)
                        .background(Color(.tertiarySystemGroupedBackground), in: .rect(cornerRadius: 18))
                    switch state {
                    case 0:
                        HStack {
                            VoiceWave()
                            Spacer()
                            Button {} label: { Image(systemName: "stop.fill").frame(width: 40, height: 40) }
                                .buttonStyle(.glassProminent).buttonBorderShape(.circle)
                        }
                    case 1: VoiceConfirm()
                    default:
                        Text("Which project on studio?").font(.headline)
                        VoiceProjectChoices()
                        VoiceWave(height: 18)
                    }
                }
            }
            .padding(20)
            .padding(.bottom, 14)
            .background(Color(.secondarySystemGroupedBackground), in: .rect(cornerRadius: 34))
            .padding(.horizontal, 8)
        }
        .ignoresSafeArea(edges: .bottom)
    }
}

// MARK: B: the form fills itself in

/// A full screen with the plan first: big rows that tick off as you
/// speak, and the sentence small at the bottom, as a caption.
struct VoiceChecklist: View {
    var state: Int

    private struct Item: Identifiable {
        let id = UUID()
        var label: String
        var value: String?
        var slot: VoiceSlot?
    }

    private var items: [Item] {
        [
            Item(label: "On", value: "studio", slot: .computer),
            Item(label: "In", value: state == 2 ? nil : "sheprd"),
            Item(label: "Branch", value: VoiceSample.branch, slot: .topic),
            Item(label: "Agent", value: state == 0 ? nil : "Claude", slot: .agent),
            Item(label: "Asked to", value: state == 0 ? nil : VoiceSample.task, slot: .task),
        ]
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text(state == 3 ? "Started" : state == 2 ? "One thing" : state == 1 ? "Ready" : "Listening")
                .font(.largeTitle.bold())
                .padding(.bottom, 26)
            if state == 3 {
                VoiceDone()
                Spacer()
            } else {
                ForEach(items) { item in
                    HStack(alignment: .firstTextBaseline, spacing: 14) {
                        Image(systemName: item.value == nil ? "circle.dotted" : "checkmark.circle.fill")
                            .foregroundStyle(item.value == nil ? AnyShapeStyle(.tertiary) : AnyShapeStyle(item.slot?.tint ?? .accentColor))
                        VStack(alignment: .leading, spacing: 2) {
                            Text(item.label).font(.subheadline).foregroundStyle(.secondary)
                            Text(item.value ?? "…").font(.title3.weight(.semibold))
                                .foregroundStyle(item.value == nil ? AnyShapeStyle(.tertiary) : AnyShapeStyle(.primary))
                        }
                    }
                    .padding(.vertical, 10)
                }
                Spacer()
                if state == 2 {
                    Text("Which project on studio?").font(.headline).padding(.bottom, 8)
                    VoiceProjectChoices().padding(.bottom, 14)
                }
                VoiceSample.text(state == 0 ? VoiceSample.partial : VoiceSample.whole)
                    .font(.footnote).foregroundStyle(.secondary).padding(.bottom, 14)
                if state == 1 {
                    VoiceConfirm()
                } else {
                    VoiceWave().frame(maxWidth: .infinity)
                }
            }
        }
        .padding(24)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .background(Color(.systemGroupedBackground))
    }
}

// MARK: C: ambient, the app stays in view

/// The app stays readable behind a glow around the screen's edge. A glass
/// capsule carries the sentence, and a card rises from it only once there
/// is a plan.
struct VoiceAmbient: View {
    var state: Int

    private var glow: AngularGradient {
        AngularGradient(colors: [.blue, .purple, .pink, .orange, .blue], center: .center)
    }

    var body: some View {
        ZStack(alignment: .bottom) {
            HomeAttention()
            if state != 3 {
                RoundedRectangle(cornerRadius: 54)
                    .strokeBorder(glow, lineWidth: 9)
                    .blur(radius: 9)
                    .ignoresSafeArea()
                    .allowsHitTesting(false)
            }
            VStack(spacing: 10) {
                if state == 1 || state == 2 {
                    VStack(alignment: .leading, spacing: 12) {
                        VoicePlanRows(state: state)
                        if state == 1 { VoiceConfirm() } else {
                            Text("Which project on studio?").font(.headline)
                            VoiceProjectChoices()
                        }
                    }
                    .padding(16)
                    .glassEffect(.regular, in: .rect(cornerRadius: 28))
                }
                if state == 3 {
                    HStack(spacing: 12) {
                        Image(systemName: "checkmark.circle.fill").font(.title2).foregroundStyle(.green)
                        VStack(alignment: .leading) {
                            Text("Claude is on it").font(.headline)
                            Text("sheprd on studio").font(.subheadline).foregroundStyle(.secondary)
                        }
                        Spacer()
                        Button("Open") {}.buttonStyle(.glassProminent)
                    }
                    .padding(14)
                    .glassEffect(.regular, in: .rect(cornerRadius: 26))
                } else {
                    HStack(alignment: .center, spacing: 12) {
                        VoiceWave(height: 22).frame(width: 60).clipped()
                        VoiceSample.text(state == 0 ? VoiceSample.partial : VoiceSample.whole)
                            .font(.subheadline).lineLimit(state == 0 ? 3 : 2).truncationMode(.head)
                        Spacer(minLength: 0)
                    }
                    .padding(14)
                    .glassEffect(.regular, in: .rect(cornerRadius: 26))
                }
            }
            .padding(.horizontal, 12)
            .padding(.bottom, 92)
        }
    }
}

// MARK: D: the sentence is the interface

/// Large type on a clean page. What the app understood turns into pills
/// inside the sentence, and tapping one changes it. There is no separate
/// form, so what you said and what will run are the same thing.
struct VoiceSentence: View {
    var state: Int

    private struct Pill: View {
        var text: String
        var slot: VoiceSlot?
        var missing = false

        var body: some View {
            Text(text)
                .font(.title2.weight(.semibold))
                .foregroundStyle(missing ? AnyShapeStyle(.orange) : AnyShapeStyle(slot?.tint ?? .primary))
                .padding(.horizontal, 12).padding(.vertical, 5)
                .background((missing ? Color.orange : slot?.tint ?? .gray).opacity(0.16), in: .capsule)
        }
    }

    private struct Word: View {
        var text: String
        var body: some View { Text(text).font(.title2).foregroundStyle(.secondary) }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            if state == 3 {
                Spacer()
                VoiceDone()
                Spacer()
            } else {
                Spacer().frame(height: 40)
                VStack(alignment: .leading, spacing: 12) {
                    HStack(spacing: 8) { Word(text: "New worktree on"); Pill(text: "studio", slot: .computer) }
                    HStack(spacing: 8) {
                        Word(text: "in")
                        if state == 2 { Pill(text: "which project?", slot: nil, missing: true) } else { Pill(text: "sheprd", slot: nil) }
                    }
                    HStack(spacing: 8) { Word(text: "about"); Pill(text: "login redirect", slot: .topic) }
                    if state == 0 {
                        Word(text: "and spin up…").opacity(0.6)
                    } else {
                        HStack(spacing: 8) { Word(text: "with"); Pill(text: "Claude", slot: .agent) }
                        Word(text: "asking it to")
                        Text("fix the redirect loop after OAuth")
                            .font(.title2.weight(.semibold))
                            .foregroundStyle(VoiceSlot.task.tint)
                            .padding(12)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .background(VoiceSlot.task.tint.opacity(0.14), in: .rect(cornerRadius: 16))
                    }
                }
                Spacer()
                if state == 2 { VoiceProjectChoices() }
                if state == 1 { VoiceConfirm() } else { VoiceWave(height: 36).frame(maxWidth: .infinity) }
            }
        }
        .padding(24)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .background(Color(.systemBackground))
    }
}
