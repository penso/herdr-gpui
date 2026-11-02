import SwiftUI

// Terminal tabs with the web picks (two key rows, action sheet, fit to
// width). A: full bleed with the keys docked. B: pane switcher and the
// action sheet open. C: a line composer with recent commands.

private struct Run {
    var text: String
    var color: Color = Color(white: 0.86)
    var bold = false
}

private let cyan = Color(red: 0.4, green: 0.8, blue: 0.9)
private let green = Color(red: 0.45, green: 0.85, blue: 0.45)
private let red = Color(red: 1, green: 0.42, blue: 0.4)
private let dim = Color(white: 0.55)
private let yellow = Color(red: 0.95, green: 0.8, blue: 0.4)

private let screen: [[Run]] = [
    [Run(text: "~/code/herdr-gpui ", color: cyan), Run(text: "mobile-app ", color: .purple), Run(text: "❯ ", color: green), Run(text: "just ci")],
    [Run(text: "cargo fmt --all -- --check", color: dim)],
    [Run(text: "cargo clippy --workspace --all-targets --locked", color: dim)],
    [Run(text: "    Checking ", color: green, bold: true), Run(text: "herdr-commands v0.42.0")],
    [Run(text: "    Checking ", color: green, bold: true), Run(text: "herdr-gpui v0.42.0")],
    [Run(text: "    Finished ", color: green, bold: true), Run(text: "`dev` profile in 41.82s")],
    [Run(text: "cargo nextest run --workspace --locked", color: dim)],
    [Run(text: "    Starting ", color: green, bold: true), Run(text: "2184 tests across 31 binaries")],
    [Run(text: "        PASS ", color: green, bold: true), Run(text: "[ 0.012s] ", color: dim), Run(text: "splits::equalize_nested")],
    [Run(text: "        PASS ", color: green, bold: true), Run(text: "[ 0.009s] ", color: dim), Run(text: "splits::equalize_single")],
    [Run(text: "        FAIL ", color: red, bold: true), Run(text: "[ 0.144s] ", color: dim), Run(text: "keymap::pane_keys::equalize")],
    [Run(text: "thread 'equalize' panicked at pane_keys.rs:48:5", color: yellow)],
    [Run(text: "assertion `left == right` failed")],
    [Run(text: "  left: ", color: dim), Run(text: "Some(\"cmd-shift-=\")", color: red)],
    [Run(text: " right: ", color: dim), Run(text: "Some(\"cmd-alt-=\")", color: green)],
    [Run(text: "     Summary ", color: red, bold: true), Run(text: "2184 run: 2183 passed, 1 failed")],
    [Run(text: "error", color: red, bold: true), Run(text: ": Recipe `test` failed with exit code 100")],
    [Run(text: "~/code/herdr-gpui ", color: cyan), Run(text: "mobile-app ", color: .purple), Run(text: "❯ ", color: red), Run(text: "█", color: Color(white: 0.8))],
]

/// The pane's screen, fit to the phone's width.
struct TerminalScreen: View {
    var fontSize: CGFloat = 10.5

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 1) {
                ForEach(Array(screen.enumerated()), id: \.offset) { _, line in
                    line.reduce(Text("")) { text, run in
                        text + Text(run.text).foregroundColor(run.color).fontWeight(run.bold ? .bold : .regular)
                    }
                    .font(.system(size: fontSize, design: .monospaced))
                    .lineLimit(1)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(10)
        }
        .defaultScrollAnchor(.bottom)
        .background(Color(red: 0.07, green: 0.07, blue: 0.09))
    }
}

private struct KeyRows: View {
    var rows: [[String]] = [
        ["esc", "tab", "ctrl", "alt", "↑", "↓", "←", "→"],
        ["~", "/", "|", "-", "$", "⌃C", "⌃D", "⌨︎"],
    ]

    var body: some View {
        VStack(spacing: 6) {
            ForEach(rows, id: \.self) { row in
                HStack(spacing: 6) {
                    ForEach(row, id: \.self) { key in
                        Text(key).font(.system(size: 14, weight: .medium, design: .monospaced))
                            .frame(maxWidth: .infinity, minHeight: 36)
                            .glassEffect(key == "ctrl" ? .regular.tint(.accentColor).interactive() : .regular.interactive(), in: .rect(cornerRadius: 9))
                    }
                }
            }
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 6)
    }
}

private struct TerminalHeader: ToolbarContent {
    var body: some ToolbarContent {
        ToolbarItem(placement: .principal) {
            VStack(spacing: 0) {
                Text("zsh — just ci").font(.headline)
                Text("MacBook Pro · herdr-gpui").font(.caption).foregroundStyle(.secondary)
            }
        }
        ToolbarItemGroup(placement: .topBarTrailing) {
            Button {} label: { Image(systemName: "doc.on.clipboard") }
            Button {} label: { Image(systemName: "ellipsis") }
        }
    }
}

struct TerminalFull: View {
    var body: some View {
        NavigationStack {
            TerminalScreen()
                .safeAreaInset(edge: .bottom) { KeyRows().background(.bar) }
                .navigationBarTitleDisplayMode(.inline)
                .toolbar { TerminalHeader() }
        }
        .environment(\.colorScheme, .dark)
    }
}

struct TerminalPanes: View {
    var body: some View {
        NavigationStack {
            TerminalScreen()
                .safeAreaInset(edge: .top) {
                    Picker("Pane", selection: .constant(1)) {
                        Text("1 zsh").tag(1)
                        Text("2 nvim").tag(2)
                        Text("3 logs").tag(3)
                    }
                    .pickerStyle(.segmented)
                    .padding(.horizontal, 16).padding(.vertical, 6)
                }
                .safeAreaInset(edge: .bottom) { KeyRows().background(.bar) }
                .navigationBarTitleDisplayMode(.inline)
                .toolbar { TerminalHeader() }
                .confirmationDialog("zsh — just ci", isPresented: .constant(true), titleVisibility: .visible) {
                    Button("Split right") {}
                    Button("Split down") {}
                    Button("Zoom pane") {}
                    Button("Size for phone") {}
                    Button("New tab") {}
                    Button("Close pane", role: .destructive) {}
                }
        }
        .environment(\.colorScheme, .dark)
    }
}

struct TerminalLine: View {
    var body: some View {
        NavigationStack {
            TerminalScreen(fontSize: 10)
                .safeAreaInset(edge: .bottom) {
                    VStack(spacing: 8) {
                        ScrollView(.horizontal, showsIndicators: false) {
                            HStack(spacing: 6) {
                                ForEach(["just ci", "git status", "cargo nextest run -p herdr-gpui keymap", "git diff"], id: \.self) { command in
                                    Text(command).font(.caption.monospaced())
                                        .padding(.horizontal, 10).padding(.vertical, 6)
                                        .glassEffect(.regular.interactive(), in: .capsule)
                                }
                            }
                            .padding(.horizontal, 12)
                        }
                        KeyRows(rows: [["esc", "tab", "ctrl", "↑", "↓", "⌃C", "⌨︎"]])
                        HStack(spacing: 8) {
                            HStack {
                                Text("❯").foregroundStyle(.green)
                                Text("cargo nextest run -p herdr-gpui equalize").font(.subheadline.monospaced())
                                    .lineLimit(1)
                                Spacer()
                            }
                            .padding(.horizontal, 14)
                            .frame(height: 44)
                            .glassEffect(.regular, in: .capsule)
                            Button {} label: { Image(systemName: "return").font(.body.bold()).frame(width: 36, height: 36) }
                                .buttonStyle(.glassProminent)
                                .buttonBorderShape(.circle)
                        }
                        .padding(.horizontal, 12)
                    }
                    .padding(.bottom, 6)
                    .background(.bar)
                }
                .navigationBarTitleDisplayMode(.inline)
                .toolbar { TerminalHeader() }
        }
        .environment(\.colorScheme, .dark)
    }
}
