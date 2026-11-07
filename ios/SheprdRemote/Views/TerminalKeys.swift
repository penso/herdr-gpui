import SheprdRemoteCore
import SwiftUI
import UIKit

/// Sends what is typed into one terminal, in order: one request at a
/// time, with keys typed meanwhile gathered into the next, so fast
/// typing neither reorders nor floods the computer.
@MainActor
@Observable
final class TermTyping {
    let key: PaneKey
    /// Held by the key row: ctrl and alt for the next key, shift until
    /// tapped off.
    var modifiers: TermInput.Modifiers = []
    private(set) var problem: String?
    /// What was typed but is not on the screen yet.
    private(set) var echo = TermEcho()
    /// What an input method is composing, typed once a candidate is picked.
    var composing = ""
    private var pending: [TermInput.Send] = []
    private var draining = false
    private weak var fleet: Fleet?

    init(key: PaneKey, fleet: Fleet) {
        self.key = key
        self.fleet = fleet
    }

    /// Typed text, with the held modifiers, which are then released. The
    /// keyboard has already applied its own shift.
    func type(_ text: String) {
        enqueue(TermInput.sends(for: text, modifiers: modifiers.subtracting(.shift)))
        modifiers = modifiers.afterKey
    }

    /// A named key (`esc`, `up`) or a combination (`ctrl+c`), with the held
    /// modifiers added.
    func press(_ name: String) {
        enqueue([.keys([TermInput.pressed(name, held: modifiers)])])
        modifiers = modifiers.afterKey
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

/// Terminal B's two key rows: the modifiers, then the person's keys, set
/// in Settings (`TermKeyLayout`): the keys a phone keyboard lacks, ^C,
/// dictation, and the keyboard toggle.
struct TermKeyRows: View {
    @Environment(AppModel.self) private var model
    @Bindable var typing: TermTyping
    @Binding var keyboard: Bool
    let dictation: Dictation
    let dictate: () -> Void

    var body: some View {
        let keys = model.termKeys.layout
        VStack(spacing: 6) {
            // What an input method composes takes the top row's place, so
            // it covers no terminal line and the terminal keeps its size.
            if typing.composing.isEmpty {
                row {
                    modifier("ctrl", .ctrl)
                    modifier("alt", .alt)
                    modifier("shift", .shift)
                    ForEach(keys.top) { key in termKey(key) }
                }
            } else {
                composing(typing.composing)
            }
            row {
                ForEach(keys.bottom) { key in termKey(key) }
            }
        }
        .padding(.horizontal, 8).padding(.vertical, 6)
        // Under the keyboard too: its rounded top corners would otherwise
        // show the window's light background beside the dark rows.
        .background { Rectangle().fill(.bar).ignoresSafeArea() }
        .sensoryFeedback(.selection, trigger: typing.modifiers)
    }

    /// The keyboard's own mic cannot reach a terminal, which has no text
    /// for it to edit, so the row offers the composer's dictation.
    private var dictationButton: some View {
        Button(action: dictate) {
            Image(systemName: dictation.active ? "stop.circle.fill" : "mic.fill")
                .font(.callout.weight(.medium))
                .foregroundStyle(dictation.active ? AnyShapeStyle(.red) : AnyShapeStyle(.primary))
                .symbolEffect(.pulse, isActive: dictation.phase == .listening)
                .termKeyFace()
        }
        .buttonStyle(.plain)
        .disabled(dictation.phase == .preparing)
        .accessibilityLabel(dictation.active ? "Stop dictating" : "Dictate")
    }

    private func row(@ViewBuilder _ content: () -> some View) -> some View {
        TermKeyRow { content() }
    }

    /// Its keys go out together; a held ctrl or alt is for the first.
    @ViewBuilder
    private func termKey(_ key: TermKey) -> some View {
        switch key.special {
        case .dictation:
            dictationButton
        case .keyboard:
            keyButton(keyboard ? "⌨︎↓" : "⌨︎", repeats: false) { keyboard.toggle() }
                .accessibilityLabel(keyboard ? "Hide keyboard" : "Show keyboard")
        case nil:
            keyButton(key.label(symbols: model.termKeys.symbols), tint: key.keys == ["ctrl+c"] ? .red : nil, repeats: key.repeats) {
                for name in key.keys { typing.press(name) }
            }
            .accessibilityLabel(key.spoken)
        }
    }

    private func keyButton(_ label: String, tint: Color? = nil, repeats: Bool = true, action: @escaping () -> Void) -> some View {
        Button(action: action) { TermKeyCap(label: label, tint: tint) }
            .buttonStyle(.plain)
            .buttonRepeatBehavior(repeats ? .enabled : .disabled)
    }

    private func composing(_ text: String) -> some View {
        HStack(spacing: 0) {
            Text(text).underline(pattern: .dot)
            Text("▏").foregroundStyle(.tint)
        }
        .font(.system(.callout, design: .monospaced))
        .lineLimit(1)
        .truncationMode(.head)
        .frame(maxWidth: .infinity, minHeight: 34, alignment: .leading)
        .padding(.horizontal, 4)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Composing: \(text)")
    }

    private func modifier(_ label: String, _ flag: TermInput.Modifiers) -> some View {
        let held = typing.modifiers.contains(flag)
        return Button {
            if held { typing.modifiers.remove(flag) } else { typing.modifiers.insert(flag) }
        } label: {
            TermKeyCap(label: TermKey.modifier(label, symbols: model.termKeys.symbols), modifier: true, held: held)
        }
        .buttonStyle(.plain)
        .accessibilityLabel(label)
        .accessibilityAddTraits(held ? .isSelected : [])
    }
}

/// One key row, shared by the terminal and the preview in Settings: its
/// keys share the row's width when they all fit, as a keyboard's do, and
/// scroll when they do not.
struct TermKeyRow<Keys: View>: View {
    @ViewBuilder let keys: Keys

    var body: some View {
        ViewThatFits(in: .horizontal) {
            HStack(spacing: 6) { keys }
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 6) { keys }
            }
        }
    }
}

/// A key row key's face, shared by the rows and their preview in Settings.
struct TermKeyCap: View {
    let label: String
    var tint: Color?
    /// The modifiers are bolder, and filled while held.
    var modifier = false
    var held = false

    var body: some View {
        Text(label).font(.system(.callout, design: .monospaced).weight(modifier ? .semibold : .medium))
            .foregroundStyle(held ? AnyShapeStyle(.white) : AnyShapeStyle(tint ?? .primary))
            .termKeyFace(held: held)
    }
}

extension View {
    /// A key's size and fill: about as narrow as a keyboard's letter key,
    /// so a row holds about ten, and wider to fill a row that holds them.
    func termKeyFace(held: Bool = false) -> some View {
        frame(minWidth: 24, minHeight: 34)
            // Never narrower than its label, so a filled row shares only
            // the room left over and `shift` stays on one line.
            .fixedSize(horizontal: true, vertical: false)
            .frame(maxWidth: .infinity)
            .padding(.horizontal, 4)
            .background(held ? AnyShapeStyle(.tint) : AnyShapeStyle(.fill.tertiary), in: .rect(cornerRadius: 8))
    }
}

/// An invisible text input that owns the keyboard: software keys arrive as
/// text, and a hardware keyboard's arrows, Escape, Tab, and Control or
/// Option combinations as key presses. Input methods compose in marked
/// text (`TermInputView+TextInput.swift`) and type what they commit.
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

final class TermInputView: UIView {
    var typing: TermTyping?
    var composition = TermComposition() {
        didSet { typing?.composing = composition.marked }
    }
    var markedTextStyle: [NSAttributedString.Key: Any]?
    weak var inputDelegate: (any UITextInputDelegate)?
    lazy var tokenizer: any UITextInputTokenizer = UITextInputStringTokenizer(textInput: self)

    override var canBecomeFirstResponder: Bool { true }
    var hasText: Bool { true }

    // Hiding the keyboard drops what is still being composed.
    override func resignFirstResponder() -> Bool {
        composition.mark("")
        return super.resignFirstResponder()
    }

    // A terminal wants what was typed, not what autocorrect guessed.
    var autocorrectionType: UITextAutocorrectionType = .no
    var autocapitalizationType: UITextAutocapitalizationType = .none
    var spellCheckingType: UITextSpellCheckingType = .no
    var smartQuotesType: UITextSmartQuotesType = .no
    var smartDashesType: UITextSmartDashesType = .no
    var smartInsertDeleteType: UITextSmartInsertDeleteType = .no
    // `.asciiCapable` would leave out the Chinese, Japanese, and Korean
    // keyboards.
    var keyboardType: UIKeyboardType = .default
    var returnKeyType: UIReturnKeyType = .default

    func insertText(_ text: String) {
        if let text = composition.insert(text) { typing?.type(text) }
    }

    func deleteBackward() {
        if composition.deleteBackward() { typing?.press("backspace") }
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
            // While composing, plain arrows, Tab, and Escape pick candidates;
            // Control and Option combinations (^C) still reach the pane.
            guard let key = press.key, composition.marked.isEmpty || !key.modifierFlags.isDisjoint(with: [.control, .alternate]),
                  let name = Self.name(for: key)
            else {
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
