import SheprdRemoteCore
import SwiftUI
import UIKit

/// A long press on the terminal and the drag after it, as SwiftTerm
/// selects: UIKit's recognizer reports where the press began and where the
/// finger moves, which SwiftUI's long press does not.
struct TermLongPress: UIGestureRecognizerRepresentable {
    /// The phase, and the finger's place in the rows and on the screen.
    let changed: (UIGestureRecognizer.State, CGPoint, CGPoint) -> Void

    func makeUIGestureRecognizer(context: Context) -> UILongPressGestureRecognizer {
        let recognizer = UILongPressGestureRecognizer()
        recognizer.minimumPressDuration = 0.4
        return recognizer
    }

    func handleUIGestureRecognizerAction(_ recognizer: UILongPressGestureRecognizer, context: Context) {
        changed(recognizer.state, context.converter.localLocation, context.converter.location(in: .global))
    }
}

/// The copy menu where a selection ends. Its actions run here rather than
/// through the responder chain, so the terminal's input view keeps the
/// keyboard.
struct TermMenu: UIViewRepresentable {
    /// Where to show it, on the screen; each new value shows it again.
    let at: CGPoint?
    let shows: Int
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
        guard let at, view.shown != shows else { return }
        view.shown = shows
        view.present(at: view.convert(at, from: nil))
    }
}

final class TermMenuView: UIView, @MainActor UIEditMenuInteractionDelegate {
    var items: [UIMenuElement] = []
    var shown = 0
    private lazy var interaction = UIEditMenuInteraction(delegate: self)

    func present(at point: CGPoint) {
        if interaction.view == nil { addInteraction(interaction) }
        interaction.presentEditMenu(with: UIEditMenuConfiguration(identifier: nil, sourcePoint: point))
    }

    func editMenuInteraction(_ interaction: UIEditMenuInteraction, menuFor configuration: UIEditMenuConfiguration,
                             suggestedActions: [UIMenuElement]) -> UIMenu? {
        UIMenu(children: items)
    }
}
