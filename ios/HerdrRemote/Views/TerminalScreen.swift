import HerdrRemoteCore
import PhotosUI
import SwiftUI
import UIKit

/// Terminal A: the pane's screen full bleed, fit to the width and pinched
/// larger or smaller, with the two key rows docked above the keyboard,
/// paste and pictures in the header, the tab's other panes as chips, and
/// the Herdr action sheet.
struct TerminalScreen: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let key: PaneKey
    let title: String
    @State private var store: TermStore?
    @State private var typing: TermTyping?
    @State private var keyboard = false
    /// Pinch zoom over the size that fits the width.
    @State private var zoom: CGFloat = 1
    @GestureState private var pinch: CGFloat = 1
    @State private var picks: [PhotosPickerItem] = []
    @State private var choosingPhotos = false
    @State private var problem: String?
    @State private var clipboardHasText = UIPasteboard.general.hasStrings
    @State private var renaming = false
    @State private var newName = ""
    /// Lines a swipe in progress has scrolled the terminal so far.
    @State private var swiped = 0
    /// The coast after a swipe lets go, sending what remains.
    @State private var coasting: Task<Void, Never>?
    /// The room the screen has, which sets the terminal's size.
    @State private var space: CGSize = .zero

    var body: some View {
        GeometryReader { geometry in
            if let screen = store?.screen {
                let fitted = fit(columns: screen.columns, width: geometry.size.width - 16)
                let size = min(28, max(4, fitted * zoom * pinch))
                let paper = Self.paper(screen)
                // Held at the phone's size and all on screen: a swipe
                // scrolls the terminal's own history instead of this view.
                let fits = store?.size.map { screen.columns <= $0.cols } == true
                    && CGFloat(screen.rows.count) * Self.lineHeight(size) + 16 <= geometry.size.height + 1
                ScrollView(.vertical) {
                    // Sideways only once pinched wider than the screen.
                    ScrollView(.horizontal, showsIndicators: false) {
                        VStack(alignment: .leading, spacing: 0) {
                            ForEach(Array(screen.rows.enumerated()), id: \.offset) { _, row in
                                Text(row.reduce(into: AttributedString()) { line, run in line += Self.styled(run) })
                                    .font(.system(size: size, design: .monospaced))
                                    .lineLimit(1)
                                    .fixedSize(horizontal: true, vertical: false)
                                    // Backgrounds are cells painted the full
                                    // line height, so blocks have no seams;
                                    // the text above stays selectable.
                                    .background(alignment: .leading) { Self.cells(row, size: size) }
                                    .frame(maxWidth: .infinity, alignment: .leading)
                                    .background(Self.rowBackground(row) ?? paper)
                            }
                        }
                        .padding(8)
                        .textSelection(.enabled)
                    }
                    .scrollDisabled(size <= fitted)
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
                // A screen that fits starts at the top; a taller one opens
                // on its last line.
                .defaultScrollAnchor(.top, for: .alignment)
                .defaultScrollAnchor(.bottom, for: .initialOffset)
                .scrollDisabled(fits)
                .simultaneousGesture(swipe(line: Self.lineHeight(size)), including: fits ? .all : .subviews)
                .background(paper)
                .gesture(
                    MagnifyGesture()
                        .updating($pinch) { value, state, _ in state = value.magnification }
                        .onEnded { value in zoom = min(3, max(0.5, zoom * value.magnification)) }
                )
                .onTapGesture(count: 2) { withAnimation(.snappy) { zoom = 1 } }
                // A tap types: it brings the keyboard up, and takes it down.
                .onTapGesture { keyboard.toggle() }
            } else {
                ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        // Measured here, inside the key rows and the bars, so the rows
        // asked for fit what is left.
        .onGeometryChange(for: CGSize.self) { $0.size } action: { space = $0 }
        // Over the terminal's last lines, not above the keys, so showing
        // them never changes the room the terminal is sized to.
        .overlay(alignment: .bottom) {
            if let typing {
                VStack(spacing: 0) {
                    if let reason = problem ?? typing.problem {
                        Text(reason).font(.footnote).foregroundStyle(.orange).padding(6)
                            .frame(maxWidth: .infinity).background(.bar)
                    }
                    let echo = typing.echo.text
                    if !echo.isEmpty { TermEchoStrip(text: echo) }
                }
            }
        }
        .background(Self.background)
        .background { if let typing { TermKeyboard(typing: typing, shown: $keyboard).frame(width: 1, height: 1).opacity(0) } }
        .safeAreaInset(edge: .top, spacing: 0) { paneChips }
        .safeAreaInset(edge: .bottom, spacing: 0) {
            if let typing {
                TermKeyRows(typing: typing, keyboard: $keyboard)
            }
        }
        .environment(\.colorScheme, .dark)
        .toolbarColorScheme(.dark, for: .navigationBar)
        // Window titles show in Stage Manager and the app switcher.
        .navigationTitle(LockState.shared.hidesNames ? "Herdr Remote" : name)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar(.hidden, for: .tabBar)
        .toolbar {
            // `PasteButton` draws its own tinted fill and greys out with
            // nothing to paste, so it stands apart from the glass group and
            // only while the clipboard holds text.
            if clipboardHasText {
                ToolbarItem(placement: .topBarTrailing) {
                    PasteButton(payloadType: String.self) { strings in
                        typing?.paste(strings.joined(separator: "\n"))
                    }
                    .labelStyle(.iconOnly)
                    .buttonBorderShape(.circle)
                }
                .sharedBackgroundVisibility(.hidden)
                ToolbarSpacer(.fixed, placement: .topBarTrailing)
            }
            ToolbarItemGroup(placement: .topBarTrailing) {
                Button { choosingPhotos = true } label: { Image(systemName: "photo") }
                    .accessibilityLabel("Send a picture")
                // A menu grows out of the button it opens from; a
                // confirmation dialog would float in the middle instead.
                Menu {
                    Section {
                        Button("Split Right", systemImage: "rectangle.split.2x1") { run(.splitRight) }
                        Button("Split Down", systemImage: "rectangle.split.1x2") { run(.splitDown) }
                        if item?.terminal.zoomed == true {
                            Button("Unzoom", systemImage: "arrow.down.right.and.arrow.up.left") { run(.zoom) }
                        } else {
                            Button("Zoom", systemImage: "arrow.up.left.and.arrow.down.right") { run(.zoom) }
                        }
                    }
                    Section {
                        Button("New Tab", systemImage: "plus") { run(.newTab) }
                        Button("Rename Tab…", systemImage: "pencil") {
                            newName = item?.terminal.label ?? title
                            renaming = true
                        }
                        if let item {
                            Button("New Chat in \(item.workspace.label)", systemImage: "bubble.left.and.text.bubble.right") {
                                model.creating = .chat(device: key.device, workspace: item.workspace.id, label: item.workspace.label)
                            }
                        }
                    }
                    Section {
                        Button("Close Pane", systemImage: "xmark.square", role: .destructive) { run(.closePane) }
                        Button("Close Tab", systemImage: "xmark", role: .destructive) { run(.closeTab) }
                    }
                } label: {
                    Image(systemName: "diamond.fill")
                }
                .accessibilityLabel("Herdr actions")
            }
        }
        .alert("Rename Tab", isPresented: $renaming) {
            TextField("Name", text: $newName)
                .textInputAutocapitalization(.never)
            Button("Cancel", role: .cancel) {}
            Button("Rename") { rename(newName) }
                .disabled(newName.trimmingCharacters(in: .whitespaces).isEmpty)
        } message: {
            Text("The name shows in Herdr and in your lists.")
        }
        .photosPicker(isPresented: $choosingPhotos, selection: $picks, maxSelectionCount: 1, matching: .images)
        .onChange(of: picks) { _, chosen in
            guard let item = chosen.first else { return }
            picks = []
            Task { await upload(item) }
        }
        .onAppear {
            store = model.fleet.openTerm(key)
            if typing == nil { typing = TermTyping(key: key, fleet: model.fleet) }
            #if DEBUG
            // The simulator loop's `-type TEXT` types into the terminal.
            if let text = UserDefaults.standard.string(forKey: "type") { typing?.type(text) }
            #endif
        }
        .task(id: space) {
            // Settles first, so a keyboard sliding in resizes once.
            try? await Task.sleep(for: .milliseconds(250))
            guard !Task.isCancelled, let store, let size = Self.termSize(fitting: space) else { return }
            model.fleet.sizeTerm(store, to: size)
        }
        .onReceive(NotificationCenter.default.publisher(for: UIPasteboard.changedNotification)) { _ in
            clipboardHasText = UIPasteboard.general.hasStrings
        }
        .onReceive(NotificationCenter.default.publisher(for: UIApplication.didBecomeActiveNotification)) { _ in
            clipboardHasText = UIPasteboard.general.hasStrings
        }
        .onDisappear {
            keyboard = false
            if let store { model.fleet.closeTerm(store) }
        }
    }

    /// The tab's name as the computer last sent it, so a rename shows at
    /// once; the route's title until the home arrives.
    private var name: String { item?.terminal.label ?? title }

    /// The terminal tab this pane is in, from the computer's home.
    private var item: TerminalItem? {
        model.fleet.terminalItems.first { $0.computer.device == key.device && $0.terminal.panes.contains { $0.pane == key.pane } }
    }

    /// The tab's panes as chips, when it has more than one.
    @ViewBuilder
    private var paneChips: some View {
        if let item, item.terminal.panes.count > 1 {
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 6) {
                    ForEach(Array(item.terminal.panes.enumerated()), id: \.element.pane) { index, pane in
                        let current = pane.pane == key.pane
                        Button {
                            guard !current else { return }
                            model.pendingRoute = .term(PaneKey(device: key.device, pane: pane.pane), title: name)
                        } label: {
                            Text(pane.label ?? "Pane \(index + 1)").font(.caption.weight(.medium)).lineLimit(1)
                                .padding(.horizontal, 10).padding(.vertical, 5)
                                .background(current ? AnyShapeStyle(.tint.opacity(0.3)) : AnyShapeStyle(.fill.tertiary), in: .capsule)
                        }
                        .buttonStyle(.plain)
                        .accessibilityAddTraits(current ? .isSelected : [])
                    }
                }
                .padding(.horizontal, 10).padding(.vertical, 6)
            }
            .background(Self.background)
        }
    }

    // MARK: Actions

    /// Dragging down goes back into the history a line per line height;
    /// letting go coasts on as far as iOS predicts the content would.
    private func swipe(line: CGFloat) -> some Gesture {
        DragGesture(minimumDistance: 8)
            .onChanged { value in
                coasting?.cancel()
                let target = Int((value.translation.height / line).rounded(.towardZero))
                notches(target - swiped)
                swiped = target
            }
            .onEnded { value in
                let rest = (value.predictedEndTranslation.height - value.translation.height) / line
                swiped = 0
                coast(max(-Self.coastLimit, min(Self.coastLimit, Int(rest.rounded(.towardZero)))))
            }
    }

    /// Lines one wheel notch moves. A program that reads the mouse takes
    /// each scroll as one notch whatever its count, so the phone sends a
    /// notch at a time, one per line of finger travel: a short swipe goes
    /// a long way, as a trackpad's scroll does.
    private static let notch = 3.0
    /// The most notches one fling coasts.
    private static let coastLimit = 40

    /// Sends `count` notches one by one, quickly then slowing, like
    /// momentum.
    private func coast(_ count: Int) {
        guard count != 0 else { return }
        coasting = Task {
            for step in 0..<abs(count) where !Task.isCancelled {
                notches(count.signum())
                try? await Task.sleep(for: .milliseconds(25 + step * 6))
            }
        }
    }

    /// Sends `count` notches as separate scrolls, above zero going back.
    private func notches(_ count: Int) {
        for _ in 0..<abs(count) {
            model.fleet.scrollTerm(key, lines: count.signum() * Int(Self.notch))
        }
    }

    private func rename(_ label: String) {
        Task {
            do {
                try await model.fleet.rename(key, to: label)
                problem = nil
            } catch {
                problem = error.localizedDescription
            }
        }
    }

    private func run(_ action: TermAction) {
        let pane = key.pane
        Task {
            do {
                let next = try await model.fleet.call(key.device) { client, token in
                    try await client.termAction(action, pane: pane, bearer: token)
                }
                switch next {
                case nil: dismiss()
                case let next? where next != pane:
                    model.pendingRoute = .term(PaneKey(device: key.device, pane: next), title: name)
                default: break
                }
                problem = nil
            } catch {
                problem = error.localizedDescription
            }
        }
    }

    private func upload(_ item: PhotosPickerItem) async {
        guard let data = try? await item.loadTransferable(type: Data.self), let image = UIImage(data: data),
              let prepared = PreparedImage(image)
        else {
            problem = "That picture could not be read."
            return
        }
        let pane = key.pane
        do {
            try await model.fleet.call(key.device) { client, token in
                try await client.termUpload(prepared.jpeg, pane: pane, bearer: token)
            }
            problem = nil
        } catch {
            problem = error.localizedDescription
        }
    }

    // MARK: Drawing

    static let background = Color(red: 0.07, green: 0.07, blue: 0.09)

    /// The color a row's last run paints, which a full-screen program
    /// fills the rest of the line with.
    static func rowBackground(_ row: [TermScreen.Run]) -> Color? {
        guard let last = row.last else { return nil }
        return last.inverse ? (color(last.fg) ?? Color(white: 0.86)) : color(last.bg)
    }

    /// The screen's own background: the one most rows end in, so a light
    /// themed program is not framed in dark.
    static func paper(_ screen: TermScreen) -> Color {
        var counts: [String: Int] = [:]
        for row in screen.rows {
            guard let last = row.last else { continue }
            counts[(last.inverse ? last.fg : last.bg) ?? "", default: 0] += 1
        }
        guard let (name, _) = counts.max(by: { $0.value < $1.value }), !name.isEmpty else { return background }
        return color(name) ?? background
    }

    private func fit(columns: Int, width: CGFloat) -> CGFloat { Self.fit(columns: columns, width: width) }

    /// The point size at which `columns` monospaced cells fill `width`.
    static func fit(columns: Int, width: CGFloat) -> CGFloat {
        guard columns > 0 else { return 11 }
        return max(4, min(14, width / (CGFloat(columns) * Self.cellRatio)))
    }

    /// A monospaced cell's width per point of font size, measured once.
    private static let cellRatio: CGFloat = {
        let font = UIFont.monospacedSystemFont(ofSize: 100, weight: .regular)
        return ("M" as NSString).size(withAttributes: [.font: font]).width / 100
    }()

    /// The font size a phone-sized terminal is drawn at.
    static let readable: CGFloat = 12

    /// The columns and rows that fit `space` at the readable size.
    static func termSize(fitting space: CGSize) -> TermSize? {
        let cols = Int((space.width - 16) / (cellRatio * readable))
        // Rows at the size the screen is then drawn at: fit to the width,
        // so a little over `readable`.
        let drawn = fit(columns: cols, width: space.width - 16)
        let rows = Int((space.height - 16) / UIFont.monospacedSystemFont(ofSize: drawn, weight: .regular).lineHeight)
        guard cols >= 20, rows >= 5 else { return nil }
        return TermSize(cols: min(cols, 400), rows: min(rows, 200))
    }

    /// A row's run backgrounds as cells the full line height.
    static func cells(_ row: [TermScreen.Run], size: CGFloat) -> some View {
        Canvas { context, canvas in
            let width = cellRatio * size
            var x: CGFloat = 0
            for run in row {
                let span = CGFloat(run.text.count) * width
                if let fill = runBackground(run) {
                    context.fill(Path(CGRect(x: x, y: 0, width: span, height: canvas.height)), with: .color(fill))
                }
                // Hidden text (SGR 8) keeps its cells but paints nothing.
                if !run.hidden, run.text.contains(where: { block($0) != nil }) {
                    let ink = foreground(run)
                    for (index, character) in run.text.enumerated() {
                        guard let shape = block(character) else { continue }
                        let cell = CGRect(x: x + CGFloat(index) * width, y: 0, width: width, height: canvas.height)
                        context.fill(Path(shape.rect(in: cell)), with: .color(ink.opacity(shape.opacity)))
                    }
                }
                x += span
            }
        }
    }

    static func lineHeight(_ size: CGFloat) -> CGFloat {
        UIFont.monospacedSystemFont(ofSize: size, weight: .regular).lineHeight
    }

    /// A block element or vertical line, drawn as a shape filling its cell
    /// so bars and scrollbars join; a glyph stops short of the line height.
    struct Block {
        /// The part of the cell it covers, as fractions.
        var x = 0.0, y = 0.0, width = 1.0, height = 1.0
        var opacity = 1.0

        func rect(in cell: CGRect) -> CGRect {
            CGRect(x: cell.minX + cell.width * x, y: cell.minY + cell.height * y,
                   width: cell.width * width, height: cell.height * height)
        }
    }

    static func block(_ character: Character) -> Block? {
        guard let scalar = character.unicodeScalars.first, character.unicodeScalars.count == 1 else { return nil }
        switch scalar.value {
        case 0x2588: return Block()
        case 0x2580: return Block(height: 0.5)
        case 0x2594: return Block(height: 0.125)
        case 0x2581...0x2587:
            let part = Double(scalar.value - 0x2580) / 8
            return Block(y: 1 - part, height: part)
        case 0x2589...0x258F:
            return Block(width: Double(0x2590 - scalar.value) / 8)
        case 0x2590: return Block(x: 0.5, width: 0.5)
        case 0x2595: return Block(x: 0.875, width: 0.125)
        case 0x2591: return Block(opacity: 0.25)
        case 0x2592: return Block(opacity: 0.5)
        case 0x2593: return Block(opacity: 0.75)
        // Light and heavy vertical lines, plain and dashed.
        case 0x2502, 0x2506, 0x250A, 0x254E: return Block(x: 0.45, width: 0.1)
        case 0x2503, 0x2507, 0x250B, 0x254F: return Block(x: 0.4, width: 0.2)
        default: return nil
        }
    }

    private static func foreground(_ run: TermScreen.Run) -> Color {
        let ink = run.inverse ? (color(run.bg) ?? Self.background) : (color(run.fg) ?? Color(white: 0.86))
        return run.dim ? ink.opacity(0.6) : ink
    }

    private static func runBackground(_ run: TermScreen.Run) -> Color? {
        run.inverse ? (color(run.fg) ?? Color(white: 0.86)) : color(run.bg)
    }

    static func styled(_ run: TermScreen.Run) -> AttributedString {
        // Backgrounds and block elements are painted as cells behind the
        // text (`cells`), so they are blank here.
        let shown = run.hidden ? String(repeating: " ", count: run.text.count)
            : String(run.text.map { block($0) == nil ? $0 : " " })
        var text = AttributedString(shown)
        text.foregroundColor = foreground(run)
        if run.underline { text.underlineStyle = .single }
        if run.strike { text.strikethroughStyle = .single }
        var intent: InlinePresentationIntent = []
        if run.bold { intent.insert(.stronglyEmphasized) }
        if run.italic { intent.insert(.emphasized) }
        if !intent.isEmpty { text.inlinePresentationIntent = intent }
        return text
    }

    private static let palette: [String: (Double, Double, Double)] = [
        "black": (0.1, 0.1, 0.1), "red": (0.87, 0.3, 0.3), "green": (0.45, 0.8, 0.4),
        "yellow": (0.9, 0.78, 0.35), "blue": (0.35, 0.55, 0.95), "magenta": (0.78, 0.45, 0.85),
        "cyan": (0.35, 0.78, 0.85), "white": (0.85, 0.85, 0.85),
        "bright_black": (0.45, 0.45, 0.45), "bright_red": (1, 0.45, 0.42), "bright_green": (0.55, 0.92, 0.5),
        "bright_yellow": (1, 0.9, 0.5), "bright_blue": (0.5, 0.68, 1), "bright_magenta": (0.9, 0.6, 1),
        "bright_cyan": (0.5, 0.9, 0.95), "bright_white": (1, 1, 1),
    ]

    private static func color(_ name: String?) -> Color? {
        guard let name else { return nil }
        if let rgb = palette[name] { return Color(red: rgb.0, green: rgb.1, blue: rgb.2) }
        guard name.hasPrefix("#"), name.count == 7, let value = UInt32(name.dropFirst(), radix: 16) else { return nil }
        return Color(
            red: Double((value >> 16) & 0xff) / 255,
            green: Double((value >> 8) & 0xff) / 255,
            blue: Double(value & 0xff) / 255
        )
    }
}
