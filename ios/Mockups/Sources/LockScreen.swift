import SwiftUI

// Live Activities drawn on a stand-in lock screen (a real activity cannot
// be rendered inside an app). A: a question with answer buttons. B: the
// step stack. C: the Dynamic Island expanded with a permission.

private struct Wallpaper<Content: View>: View {
    @ViewBuilder var content: Content

    var body: some View {
        ZStack(alignment: .top) {
            LinearGradient(colors: [Color(red: 0.12, green: 0.14, blue: 0.32), Color(red: 0.45, green: 0.2, blue: 0.4), Color(red: 0.9, green: 0.5, blue: 0.35)],
                           startPoint: .top, endPoint: .bottom)
                .ignoresSafeArea()
            VStack(spacing: 0) {
                Text("Wednesday, October 28").font(.title3.weight(.semibold)).padding(.top, 70)
                Text("9:41").font(.system(size: 96, weight: .semibold, design: .rounded))
                Spacer()
                content
                    .padding(.horizontal, 12)
                    .padding(.bottom, 120)
            }
            .foregroundStyle(.white)
        }
        .environment(\.colorScheme, .dark)
    }
}

private struct ActivityHeader: View {
    var agent: Agent
    var title: String
    var subtitle: String
    var trailing: String
    var amber: Bool

    var body: some View {
        HStack(spacing: 10) {
            AgentIcon(agent: agent, size: 30)
            VStack(alignment: .leading, spacing: 0) {
                Text(title).font(.subheadline.weight(.semibold)).lineLimit(1)
                Text(subtitle).font(.caption).foregroundStyle(.white.opacity(0.7))
            }
            Spacer()
            Text(trailing).font(.caption.weight(.semibold))
                .foregroundStyle(amber ? .orange : .white.opacity(0.8))
        }
    }
}

struct LockQuestion: View {
    var body: some View {
        Wallpaper {
            VStack(alignment: .leading, spacing: 12) {
                ActivityHeader(agent: .claude, title: "Hero animation", subtitle: "studio · site",
                               trailing: "Needs an answer", amber: true)
                Text("Which layout should the hero use?").font(.headline)
                HStack(spacing: 8) {
                    ForEach(["Split image", "Full-bleed video", "Centered"], id: \.self) { option in
                        Button {} label: { Text(option).font(.footnote.weight(.semibold)).lineLimit(1).frame(maxWidth: .infinity) }
                            .buttonStyle(.glass)
                    }
                }
            }
            .padding(16)
            .background(.black.opacity(0.55), in: .rect(cornerRadius: 26))
        }
    }
}

struct LockStepStack: View {
    var body: some View {
        Wallpaper {
            VStack(alignment: .leading, spacing: 10) {
                ActivityHeader(agent: .claude, title: "Equalize splits command", subtitle: "MacBook Pro · herdr-gpui",
                               trailing: "4:12 · $0.84", amber: false)
                VStack(spacing: 4) {
                    StackCard(text: "Edit commands.rs", symbol: "pencil").scaleEffect(0.9).opacity(0.35)
                    StackCard(text: "Edit tests/equalize.rs", symbol: "pencil").scaleEffect(0.95).opacity(0.6)
                    StackCard(text: "Running just ci", symbol: "terminal", running: true)
                }
                HStack(spacing: 4) {
                    ForEach(0..<5) { index in
                        Capsule().fill(index < 3 ? .white : .white.opacity(0.25)).frame(height: 4)
                    }
                }
                HStack {
                    Text("Step 4 of 5").font(.caption)
                    Spacer()
                    Text("MacBook Pro: 2 working · 1 waiting").font(.caption).foregroundStyle(.white.opacity(0.7))
                }
            }
            .padding(16)
            .background(.black.opacity(0.55), in: .rect(cornerRadius: 26))
        }
    }
}

private struct StackCard: View {
    var text: String
    var symbol: String
    var running = false

    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: symbol)
            Text(text).font(.subheadline.weight(.medium))
            Spacer()
            if running { ProgressView().controlSize(.small).tint(.white) }
        }
        .padding(.horizontal, 12).padding(.vertical, 10)
        .background(Color(white: 0.18), in: .rect(cornerRadius: 14))
    }
}

struct IslandExpanded: View {
    var body: some View {
        ZStack(alignment: .top) {
            LinearGradient(colors: [Color(red: 0.2, green: 0.35, blue: 0.55), Color(red: 0.1, green: 0.15, blue: 0.25)],
                           startPoint: .top, endPoint: .bottom)
                .ignoresSafeArea()
            VStack(alignment: .leading, spacing: 12) {
                HStack(alignment: .top) {
                    AgentIcon(agent: .codex, size: 34)
                    Spacer()
                    VStack(alignment: .trailing, spacing: 0) {
                        Text("Permission").font(.caption.weight(.semibold)).foregroundStyle(.orange)
                        Text("MacBook Pro").font(.caption2).foregroundStyle(.white.opacity(0.6))
                    }
                }
                Text("Fix mise discovery on SSH").font(.subheadline.weight(.semibold))
                Text("git push origin fix/mise").font(.system(.footnote, design: .monospaced))
                    .padding(.horizontal, 10).padding(.vertical, 6)
                    .background(Color(white: 0.16), in: .rect(cornerRadius: 10))
                HStack(spacing: 8) {
                    Button {} label: { Text("Allow once").font(.subheadline.weight(.semibold)).frame(maxWidth: .infinity) }
                        .buttonStyle(.glassProminent).tint(.orange)
                    Button {} label: { Text("Deny").font(.subheadline.weight(.semibold)).frame(maxWidth: .infinity) }
                        .buttonStyle(.glass)
                    Button {} label: { Image(systemName: "faceid") }
                        .buttonStyle(.glass)
                }
            }
            .foregroundStyle(.white)
            .padding(18)
            .background(.black, in: .rect(cornerRadius: 44))
            .padding(.horizontal, 8)
            .padding(.top, 10)
            .ignoresSafeArea(edges: .top)
            Text("9:41").font(.system(size: 96, weight: .semibold, design: .rounded)).foregroundStyle(.white.opacity(0.35))
                .padding(.top, 330)
        }
        .statusBarHidden()
        .environment(\.colorScheme, .dark)
    }
}
