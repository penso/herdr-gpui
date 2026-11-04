import SheprdRemoteCore
import SwiftUI

/// What belongs to one window rather than the whole app: its pair sheet,
/// and what its connection toast gives way to. On iPad each window has its
/// own, so Pair Again or an open chat in one leaves the others alone. A
/// window without one (a chat in a window of its own) has neither.
@MainActor
@Observable
final class WindowState {
    /// The pair sheet is open; Pair Again opens it from any screen.
    var pairing = false
    /// The computer nearby whose proximity card is open.
    var nearby: NearbyComputer?
    /// That card's pairing has started and has not finished.
    var exchanging = false
    /// A Workspaces or Settings sheet covers an iPad window.
    var panelOpen = false
    /// Chats and terminals on screen: their input is at the bottom, where
    /// the connection toast would cover it.
    var typing = 0
    /// Home is on screen, with a card for each unpaired computer.
    var home = false
}
