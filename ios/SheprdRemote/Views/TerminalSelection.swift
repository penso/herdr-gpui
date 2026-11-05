import SheprdRemoteCore
import SwiftUI
import UIKit

/// Selecting the terminal's rows by cell, as iOS text views select, while
/// the terminal's input view keeps the keyboard: a long press selects the
/// word under it and its drag extends that; handles at the ends then move
/// either end; a tap inside shows the copy menu again and one outside
/// clears it. Points are the rows' own, so zoom and sideways scrolling
/// move the handles with the text.
struct TermSelecting: ViewModifier {
    let screen: TermScreen
    /// A cell's width and a row's height.
    let cell: CGSize
    @Binding var selection: TermSelection?
    let paste: () -> Void
    /// The word a long press began on, which its drag extends from.
    @State private var pressed: TermSelection?
    /// The handle the finger holds.
    @State private var held: TermHandleDrag?
    /// Where the copy menu shows in the rows, and how many times it has.
    @State private var menuAt: CGPoint?
    @State private var menuShows = 0

    func body(content: Content) -> some View {
        content
            .gesture(TermLongPress(duration: 0.4) { state, point in press(state, at: point) })
            // Only with a selection, so a tap otherwise reaches the
            // terminal's own, which brings up the keyboard.
            .gesture(SpatialTapGesture().onEnded { tap(at: $0.location) }, including: selection == nil ? .subviews : .all)
            // Over the rows and their gestures: a handle takes its touch at
            // once, so dragging it neither selects nor scrolls.
            .overlay(alignment: .topLeading) {
                if let selection {
                    let handles = TermHandles(selection, cell: cell)
                    TermHandleShape(handles: handles)
                        .fill(Color.accentColor)
                        .contentShape(TermHandleTargets(handles: handles))
                        .gesture(TermLongPress(duration: 0) { state, point in drag(state, at: point) })
                        .accessibilityHidden(true)
                }
            }
            .background {
                TermMenu(at: selection == nil ? nil : menuAt, shows: menuShows,
                         clear: selection.map { TermHandles($0, cell: cell).bounds } ?? .null) {
                    if let selection { UIPasteboard.general.string = screen.text(selection) }
                    selection = nil
                } selectAll: {
                    selection = screen.everything
                    menuShows += 1
                } paste: {
                    paste()
                    selection = nil
                }
            }
            .onDisappear { selection = nil }
    }

    /// A long press selects the word under it, its drag extends that by
    /// cell, and letting go shows the copy menu above it.
    private func press(_ state: UIGestureRecognizer.State, at point: CGPoint) {
        let at = TermSelection.Position(at: point, cell: cell)
        switch state {
        case .began:
            pressed = screen.whole(screen.word(at: at))
            selection = pressed
            menuAt = nil
        case .changed:
            guard let pressed else { return }
            selection = screen.extending(pressed, to: at)
        case .ended:
            if let selection { showMenu(at: TermHandles(selection, cell: cell).menu) }
        default:
            break
        }
    }

    /// A handle follows the finger cell by cell, the menu hidden until it
    /// lets go.
    private func drag(_ state: UIGestureRecognizer.State, at point: CGPoint) {
        switch state {
        case .began:
            held = selection.flatMap { TermHandleDrag(TermHandles($0, cell: cell), at: point) }
            menuAt = nil
        case .changed:
            guard let held else { return }
            selection = held.selection(at: point, cell: cell, in: screen)
        case .ended, .cancelled:
            held = nil
            if let selection { showMenu(at: TermHandles(selection, cell: cell).menu) }
        default:
            break
        }
    }

    private func tap(at point: CGPoint) {
        guard let selection else { return }
        if screen.highlights(selection, at: TermSelection.Position(at: point, cell: cell)) {
            showMenu(at: point)
        } else {
            self.selection = nil
        }
    }

    private func showMenu(at point: CGPoint) {
        menuAt = point
        menuShows += 1
    }
}

/// A press on the terminal and the drag after it, as SwiftTerm selects:
/// UIKit's recognizer reports where the press began and where the finger
/// moves, which SwiftUI's long press does not.
struct TermLongPress: UIGestureRecognizerRepresentable {
    /// How long the finger rests first; none takes a touch at once.
    let duration: TimeInterval
    /// The phase, and the finger's place in the view it is on.
    let changed: (UIGestureRecognizer.State, CGPoint) -> Void

    func makeUIGestureRecognizer(context: Context) -> UILongPressGestureRecognizer {
        let recognizer = UILongPressGestureRecognizer()
        recognizer.minimumPressDuration = duration
        return recognizer
    }

    func handleUIGestureRecognizerAction(_ recognizer: UILongPressGestureRecognizer, context: Context) {
        changed(recognizer.state, context.converter.localLocation)
    }
}

/// A selection's handles, in the rows: a bar and a knob at each end.
private struct TermHandleShape: Shape {
    let handles: TermHandles

    func path(in rect: CGRect) -> Path {
        var path = Path()
        for end in [TermSelection.End.start, .end] {
            path.addRect(handles.bar(end).insetBy(dx: -1, dy: 0))
            path.addEllipse(in: handles.knob(end))
        }
        return path
    }
}

/// Where the handles take a touch: a fingertip around each.
private struct TermHandleTargets: Shape {
    let handles: TermHandles

    func path(in rect: CGRect) -> Path {
        Path { $0.addRects([handles.target(.start), handles.target(.end)]) }
    }
}

/// The copy menu by a selection. Its actions run here rather than through
/// the responder chain, so the terminal's input view keeps the keyboard.
struct TermMenu: UIViewRepresentable {
    /// Where to show it, in this view; none hides it, and each new count
    /// shows it again.
    let at: CGPoint?
    let shows: Int
    /// What it keeps clear of: the selection and its handles.
    let clear: CGRect
    let copy: () -> Void
    let selectAll: () -> Void
    let paste: () -> Void

    func makeUIView(context: Context) -> TermMenuView { TermMenuView() }

    func updateUIView(_ view: TermMenuView, context: Context) {
        view.items = [
            UIAction(title: "Copy", image: UIImage(systemName: "doc.on.doc")) { _ in copy() },
            UIAction(title: "Select All", image: UIImage(systemName: "selection.pin.in.out")) { _ in selectAll() },
            UIAction(title: "Paste", image: UIImage(systemName: "doc.on.clipboard"),
                     attributes: UIPasteboard.general.hasStrings ? [] : .disabled) { _ in paste() },
        ]
        view.clear = clear
        guard let at else {
            view.dismiss()
            return
        }
        guard view.shown != shows else { return }
        view.shown = shows
        view.present(at: at)
    }
}

final class TermMenuView: UIView, @MainActor UIEditMenuInteractionDelegate {
    var items: [UIMenuElement] = []
    var shown = 0
    var clear = CGRect.null
    private lazy var interaction = UIEditMenuInteraction(delegate: self)

    func present(at point: CGPoint) {
        if interaction.view == nil { addInteraction(interaction) }
        interaction.presentEditMenu(with: UIEditMenuConfiguration(identifier: nil, sourcePoint: point))
    }

    func dismiss() {
        if interaction.view != nil { interaction.dismissMenu() }
    }

    func editMenuInteraction(_ interaction: UIEditMenuInteraction, menuFor configuration: UIEditMenuConfiguration,
                             suggestedActions: [UIMenuElement]) -> UIMenu? {
        UIMenu(children: items)
    }

    /// Above or below the selection, never over a handle.
    func editMenuInteraction(_ interaction: UIEditMenuInteraction, targetRectFor configuration: UIEditMenuConfiguration) -> CGRect {
        clear.isNull ? CGRect(origin: configuration.sourcePoint, size: .zero) : clear
    }
}
