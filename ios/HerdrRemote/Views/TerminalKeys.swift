import HerdrRemoteCore
import SwiftUI
import UIKit

/// Sends what is typed into one terminal, in order: one request at a
/// time, with keys typed meanwhile gathered into the next, so fast
/// typing neither reorders nor floods the computer.
@MainActor
@Observable
final class TermTyping {
    let key: PaneKey
    /// Held by the key row for the next key typed.
    var modifiers: TermInput.Modifiers = []
    private(set) var problem: String?
    /// What was typed but is not on the screen yet.
    private(set) var echo = TermEcho()
    private var pending: [TermInput.Send] = []
    private var draining = false
    private weak var fleet: Fleet?

    init(key: PaneKey, fleet: Fleet) {
        self.key = key
        self.fleet = fleet
    }

    /// Typed text, with the held modifiers, which are then released.
    func type(_ text: String) {
        enqueue(TermInput.sends(for: text, modifiers: modifiers))
        modifiers = []
    }

    /// A named key (`esc`, `up`) or a combination (`ctrl+c`), with the held
    /// modifiers added.
    func press(_ name: String) {
        let parts = name.split(separator: "+").map(String.init)
        var held = modifiers
        for part in parts.dropLast() {
            switch part {
            case "ctrl": held.insert(.ctrl)
            case "alt": held.insert(.alt)
            case "shift": held.insert(.shift)
            default: break
            }
        }
        enqueue([.keys([TermInput.named(parts.last ?? name, modifiers: held)])])
        modifiers = []
    }

    func paste(_ text: String) {
        guard !text.isEmpty else { return }
        enqueue([.paste(String(text.prefix(64 * 1024)))])
    }

    private func enqueue(_ sends: [TermInput.Send]) {
        for send in sends {
            echo.queue(send, at: .now)
            // Keys typed while a request is out join the last batch.
            if case let .keys(more) = send, case let .keys(queued)? = pending.last, queued.count + more.count <= TermInput.maxKeys {
                pending[pending.count - 1] = .keys(queued + more)
            } else {
                pending.append(send)
            }
        }
        guard !draining else { return }
        draining = true
        Task { await drain() }
    }

    private func drain() async {
        defer { draining = false }
        while !pending.isEmpty, let fleet {
            let send = pending.removeFirst()
            let pane = key.pane
            switch send {
            case let .keys(keys): echo.sending(keys.count, at: .now)
            case .paste: echo.sending(1, at: .now)
            }
            do {
                try await fleet.call(key.device) { client, token in
                    switch send {
                    case let .keys(keys): try await client.termKeys(keys, pane: pane, bearer: token)
                    case let .paste(text): try await client.termPaste(text, pane: pane, bearer: token)
                    }
                }
                problem = nil
                echo.delivered(at: .now)
                Task { [weak self] in
                    // Typing since pushes this back: a later delivery
                    // tries again after its own wait.
                    try? await Task.sleep(for: TermEcho.quiet)
                    self?.echo.settle(at: .now)
                }
            } catch {
                // What failed is dropped rather than replayed later into
                // a screen that has moved on.
                problem = error.localizedDescription
                pending.removeAll()
                echo.clear()
            }
        }
    }
}

/// What was typed but is not on the screen yet, above the key rows: its
/// end, dotted, as the terminal never draws text.
struct TermEchoStrip: View {
    let text: String

    var body: some View {
        HStack(spacing: 0) {
            Text(text).underline(pattern: .dot)
            Text("▏").foregroundStyle(.tint)
        }
        .font(.system(.footnote, design: .monospaced))
        .foregroundStyle(.secondary)
        .lineLimit(1)
        .truncationMode(.head)
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, 12).padding(.vertical, 6)
        // Part of the key rows below, not the terminal above: its own
        // background keeps it legible over a light themed program.
        .background(.bar)
        .accessibilityLabel("Typed, not on the screen yet: \(text)")
    }
}

/// Terminal B's two key rows: modifiers and the keys a phone keyboard
/// lacks, with ^C set apart, the arrows, and the keyboard toggle.
struct TermKeyRows: View {
    @Bindable var typing: TermTyping
    @Binding var keyboard: Bool

    private static let top: [(key: String, label: String)] = [
        ("esc", "esc"), ("tab", "⇥"), ("|", "|"), ("slash", "/"), ("minus", "-"), ("~", "~"), ("backtick", "`"), ("up", "↑"),
    ]
    private static let bottom: [(key: String, label: String)] = [
        ("ctrl+d", "^D"), ("ctrl+z", "^Z"), ("ctrl+r", "^R"), ("ctrl+l", "^L"), ("ctrl+a", "^A"), ("ctrl+e", "^E"),
        ("left", "←"), ("down", "↓"), ("right", "→"),
    ]

    var body: some View {
        VStack(spacing: 6) {
            row {
                modifier("ctrl", .ctrl)
                modifier("alt", .alt)
                ForEach(Self.top, id: \.key) { key in keyButton(key.label) { typing.press(key.key) } }
            }
            row {
                keyButton("^C", tint: .red) { typing.press("ctrl+c") }
                    .accessibilityLabel("Control C")
                ForEach(Self.bottom, id: \.key) { key in keyButton(key.label) { typing.press(key.key) } }
                keyButton(keyboard ? "⌨︎↓" : "⌨︎") { keyboard.toggle() }
                    .accessibilityLabel(keyboard ? "Hide keyboard" : "Show keyboard")
            }
        }
        .padding(.horizontal, 8).padding(.vertical, 6)
        .background(.bar)
        .sensoryFeedback(.selection, trigger: typing.modifiers)
    }

    private func row(@ViewBuilder _ content: () -> some View) -> some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 6) { content() }
        }
    }

    private func keyButton(_ label: String, tint: Color? = nil, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Text(label).font(.system(.callout, design: .monospaced).weight(.medium))
                .foregroundStyle(tint ?? .primary)
                .frame(minWidth: 38, minHeight: 34).padding(.horizontal, 4)
                .background(.fill.tertiary, in: .rect(cornerRadius: 8))
        }
        .buttonStyle(.plain)
        .buttonRepeatBehavior(.enabled)
    }

    private func modifier(_ label: String, _ flag: TermInput.Modifiers) -> some View {
        let held = typing.modifiers.contains(flag)
        return Button {
            if held { typing.modifiers.remove(flag) } else { typing.modifiers.insert(flag) }
        } label: {
            Text(label).font(.system(.callout, design: .monospaced).weight(.semibold))
                .foregroundStyle(held ? AnyShapeStyle(.white) : AnyShapeStyle(.primary))
                .frame(minWidth: 44, minHeight: 34)
                .background(held ? AnyShapeStyle(.tint) : AnyShapeStyle(.fill.tertiary), in: .rect(cornerRadius: 8))
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(held ? .isSelected : [])
    }
}

/// An invisible text input that owns the keyboard: software keys arrive as
/// text, and a hardware keyboard's arrows, Escape, Tab, and Control or
/// Option combinations as key presses.
struct TermKeyboard: UIViewRepresentable {
    let typing: TermTyping
    @Binding var shown: Bool

    func makeUIView(context: Context) -> TermInputView {
        let view = TermInputView()
        view.typing = typing
        return view
    }

    func updateUIView(_ view: TermInputView, context: Context) {
        view.typing = typing
        if shown, !view.isFirstResponder {
            DispatchQueue.main.async { view.becomeFirstResponder() }
        } else if !shown, view.isFirstResponder {
            DispatchQueue.main.async { view.resignFirstResponder() }
        }
    }
}

final class TermInputView: UIView, UIKeyInput {
    var typing: TermTyping?

    override var canBecomeFirstResponder: Bool { true }
    var hasText: Bool { true }

    // A terminal wants what was typed, not what autocorrect guessed.
    var autocorrectionType: UITextAutocorrectionType = .no
    var autocapitalizationType: UITextAutocapitalizationType = .none
    var spellCheckingType: UITextSpellCheckingType = .no
    var smartQuotesType: UITextSmartQuotesType = .no
    var smartDashesType: UITextSmartDashesType = .no
    var smartInsertDeleteType: UITextSmartInsertDeleteType = .no
    var keyboardType: UIKeyboardType = .asciiCapable
    var returnKeyType: UIReturnKeyType = .default

    func insertText(_ text: String) {
        typing?.type(text)
    }

    func deleteBackward() {
        typing?.press("backspace")
    }

    // ⌘V from a hardware keyboard, and Paste from the edit menu.
    override func canPerformAction(_ action: Selector, withSender sender: Any?) -> Bool {
        action == #selector(paste(_:)) ? UIPasteboard.general.hasStrings : super.canPerformAction(action, withSender: sender)
    }

    override func paste(_ sender: Any?) {
        if let text = UIPasteboard.general.string { typing?.paste(text) }
    }

    override func pressesBegan(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        var unhandled = Set<UIPress>()
        for press in presses {
            guard let key = press.key, let name = Self.name(for: key) else {
                unhandled.insert(press)
                continue
            }
            typing?.press(name)
        }
        if !unhandled.isEmpty { super.pressesBegan(unhandled, with: event) }
    }

    /// The Herdr key for a hardware press this view handles itself, or
    /// `nil` to let it arrive as text.
    static func name(for key: UIKey) -> String? {
        var modifiers: TermInput.Modifiers = []
        if key.modifierFlags.contains(.control) { modifiers.insert(.ctrl) }
        if key.modifierFlags.contains(.alternate) { modifiers.insert(.alt) }
        let named: String? = switch key.keyCode {
        case .keyboardUpArrow: "up"
        case .keyboardDownArrow: "down"
        case .keyboardLeftArrow: "left"
        case .keyboardRightArrow: "right"
        case .keyboardEscape: "esc"
        case .keyboardTab: "tab"
        default: nil
        }
        if let named {
            if key.modifierFlags.contains(.shift) { modifiers.insert(.shift) }
            return TermInput.named(named, modifiers: modifiers)
        }
        // ⌘ shortcuts (paste) stay with the system; plain typing is text.
        guard !modifiers.isEmpty, !key.modifierFlags.contains(.command),
              let character = key.charactersIgnoringModifiers.first
        else { return nil }
        return TermInput.key(for: character, modifiers: modifiers)
    }
}
