import Foundation

/// How a computer is reached right now.
public enum LinkStatus: Hashable, Sendable {
    case connecting
    case online(origin: String)
    /// Nothing answered, or the connection dropped; it retries by itself
    /// and stays offline until a retry gets through.
    case offline(reason: String)
    /// The computer no longer knows this phone: unpaired or revoked there.
    case revoked
    /// The computer's Herdr has no live API for this app: update Herdr
    /// there. It keeps retrying, so an update is picked up by itself.
    case outdated
}

/// What a link reports: its status, and what its socket says.
public enum LinkUpdate: Hashable, Sendable {
    case status(LinkStatus)
    case event(LiveEvent)
}

/// Failures the I/O closures report that change what the link does next.
public enum LinkError: Error, Hashable, Sendable {
    /// The computer refused this device: stop and say so.
    case unauthorized
    /// An address answered as another computer.
    case wrongComputer
    /// The computer answered, but has no live socket for this app.
    case outdated
}

/// An open live socket.
public struct LiveConnection: Sendable {
    public var events: AsyncThrowingStream<LiveEvent, Error>
    public var send: @Sendable (LiveRequest) async throws -> Void
    public var close: @Sendable () -> Void

    public init(
        events: AsyncThrowingStream<LiveEvent, Error>,
        send: @escaping @Sendable (LiveRequest) async throws -> Void,
        close: @escaping @Sendable () -> Void
    ) {
        self.events = events
        self.send = send
        self.close = close
    }
}

/// The network a link uses, injected so the link's logic is tested
/// without one.
public struct LinkIO: Sendable {
    /// The fingerprint the computer at `origin` reports (`/api/ping`).
    public var ping: @Sendable (_ origin: String) async throws -> String
    /// A bearer token from `origin`, signed with the device key.
    public var session: @Sendable (_ origin: String) async throws -> String
    public var connect: @Sendable (_ origin: String, _ token: String) async throws -> LiveConnection
    public var sleep: @Sendable (Duration) async throws -> Void

    public init(
        ping: @escaping @Sendable (String) async throws -> String,
        session: @escaping @Sendable (String) async throws -> String,
        connect: @escaping @Sendable (String, String) async throws -> LiveConnection,
        sleep: @escaping @Sendable (Duration) async throws -> Void = { try await Task.sleep(for: $0) }
    ) {
        self.ping = ping
        self.session = session
        self.connect = connect
        self.sleep = sleep
    }
}

/// One paired computer's live connection: races its addresses, keeps a
/// socket open, resubscribes what the phone watches after every reconnect
/// (chats from the cursor they reached), drops a chat's changes that skip
/// ahead and asks again, and backs off while nothing answers.
public actor ComputerLink {
    public nonisolated let updates: AsyncStream<LinkUpdate>
    private let continuation: AsyncStream<LinkUpdate>.Continuation
    private let fingerprint: String
    private var origins: [String]
    private let io: LinkIO
    /// What the phone watches, and for chats the cursor it holds.
    private var watched: [LiveView: Cursor?] = [:]
    /// The size each watched terminal asked for, sent again on reconnect.
    private var sizes: [LiveView: TermSize] = [:]
    private var connection: LiveConnection?
    private var runner: Task<Void, Never>?
    private var wake: CheckedContinuation<Void, Never>?
    private var attempt = 0
    private var pauseID = 0
    /// A network change that came while no pause was running skips the next.
    private var retryNow = false

    /// How long addresses wait for the one before them in a race.
    public static let stagger: Duration = .milliseconds(250)
    /// The longest wait between attempts.
    public static let maxBackoff: Duration = .seconds(30)

    public init(fingerprint: String, origins: [String], io: LinkIO) {
        (updates, continuation) = AsyncStream.makeStream(bufferingPolicy: .bufferingNewest(256))
        self.fingerprint = Self.normalized(fingerprint)
        self.origins = origins
        self.io = io
    }

    public func start() {
        guard runner == nil else { return }
        runner = Task { await run() }
    }

    public func stop() {
        runner?.cancel()
        runner = nil
        connection?.close()
        connection = nil
        wakeUp()
        continuation.finish()
    }

    /// Watches `view`; for a chat, `since` is the cursor of the cached
    /// transcript, so an unchanged chat costs no snapshot.
    public func watch(_ view: LiveView, since: Cursor? = nil, size: TermSize? = nil) async {
        watched[view] = .some(since)
        sizes[view] = size
        try? await connection?.send(.subscribe(view, since: since, size: size))
    }

    /// Scrolls a watched terminal; dropped while the link is down, as a
    /// scroll is only worth anything while the person watches.
    public func scroll(pane: String, lines: Int) async {
        guard lines != 0, watched[.term(pane: pane)] != nil else { return }
        try? await connection?.send(.termScroll(pane: pane, lines: lines))
    }

    public func unwatch(_ view: LiveView) async {
        guard watched.removeValue(forKey: view) != nil else { return }
        sizes[view] = nil
        try? await connection?.send(.unsubscribe(view))
    }

    /// The network changed: stop waiting and try every address again.
    public func networkChanged() {
        attempt = 0
        if wake == nil { retryNow = true }
        wakeUp()
    }

    public func updateOrigins(_ origins: [String]) {
        self.origins = origins
    }

    private func run() async {
        // Once offline, a retry stays offline until it gets through, so
        // what the phone shows does not flicker with every attempt.
        var offline = false
        while !Task.isCancelled {
            if !offline { continuation.yield(.status(.connecting)) }
            do {
                guard let origin = await race() else {
                    throw Offline(reason: "No address answered (\(origins.count) tried)")
                }
                let token = try await io.session(origin)
                let opened = try await io.connect(origin, token)
                connection = opened
                attempt = 0
                retryNow = false
                offline = false
                continuation.yield(.status(.online(origin: origin)))
                for (view, since) in watched {
                    try await opened.send(.subscribe(view, since: since, size: sizes[view]))
                }
                let ending = try await follow(opened)
                connection = nil
                opened.close()
                if ending == .revoked {
                    continuation.yield(.status(.revoked))
                    return
                }
                // An expired session reconnects at once with a new token.
                if ending == .expired { continue }
                throw Offline(reason: "The connection closed")
            } catch LinkError.unauthorized {
                continuation.yield(.status(.revoked))
                return
            } catch LinkError.outdated {
                connection = nil
                guard !Task.isCancelled else { return }
                continuation.yield(.status(.outdated))
                offline = true
                await pause()
            } catch {
                connection = nil
                guard !Task.isCancelled else { return }
                let reason = (error as? Offline)?.reason ?? "The connection failed"
                continuation.yield(.status(.offline(reason: reason)))
                offline = true
                await pause()
            }
        }
    }

    private struct Offline: Error {
        let reason: String
    }

    private enum Ending { case closed, expired, revoked }

    /// Passes events on until the socket ends; keeps chat cursors.
    private func follow(_ opened: LiveConnection) async throws -> Ending {
        for try await event in opened.events {
            switch event {
            case let .bye(reason):
                return reason == "revoked" ? .revoked : .expired
            case let .chatSnapshot(snapshot):
                if watched[.chat(pane: snapshot.pane)] != nil {
                    watched[.chat(pane: snapshot.pane)] = .some(snapshot.cursor)
                }
            case let .chatResumed(pane, cursor, _):
                if watched[.chat(pane: pane)] != nil { watched[.chat(pane: pane)] = .some(cursor) }
            case let .chatChanges(pane, changes):
                let view = LiveView.chat(pane: pane)
                guard let held = watched[view], let last = changes.last else { continue }
                if let held, let first = changes.first, !held.isFollowed(by: first.cursor) {
                    // A gap: what follows does not apply to what the phone
                    // holds. Ask again from where it is.
                    try await opened.send(.subscribe(view, since: held))
                    continue
                }
                watched[view] = .some(last.cursor)
            case let .reload(view):
                if watched[view] != nil {
                    watched[view] = .some(nil)
                    try await opened.send(.subscribe(view, since: nil, size: sizes[view]))
                }
                continue
            default:
                break
            }
            continuation.yield(.event(event))
        }
        return .closed
    }

    /// The first address that answers as this computer, trying them in
    /// order with a short head start each.
    private func race() async -> String? {
        let candidates = origins
        let io = io
        let fingerprint = fingerprint
        return await withTaskGroup(of: String?.self) { group in
            for (index, origin) in candidates.enumerated() {
                group.addTask {
                    if index > 0 { try? await io.sleep(Self.stagger * index) }
                    guard !Task.isCancelled, let answer = try? await io.ping(origin) else { return nil }
                    return Self.normalized(answer) == fingerprint ? origin : nil
                }
            }
            for await found in group {
                if let found {
                    group.cancelAll()
                    return found
                }
            }
            return nil
        }
    }

    /// Waits 1, 1.5, 2.25… seconds up to the cap, or until woken.
    private func pause() async {
        if retryNow {
            retryNow = false
            return
        }
        let delay = Self.backoff(attempt: attempt)
        attempt += 1
        pauseID += 1
        let id = pauseID
        let io = io
        await withCheckedContinuation { (resume: CheckedContinuation<Void, Never>) in
            wake = resume
            Task {
                try? await io.sleep(delay)
                self.wakeUp(id)
            }
        }
    }

    /// Ends the current pause; a sleep that outlived its pause wakes nothing.
    private func wakeUp(_ id: Int? = nil) {
        if let id, id != pauseID { return }
        wake?.resume()
        wake = nil
    }

    /// A fingerprint as hex digits alone: `/api/ping` writes it with colons.
    static func normalized(_ fingerprint: String) -> String {
        fingerprint.uppercased().filter { $0 != ":" }
    }

    /// The wait after `attempt` failures in a row.
    public static func backoff(attempt: Int) -> Duration {
        .milliseconds(Int(min(pow(1.5, Double(attempt)), Double(maxBackoff.components.seconds)) * 1000))
    }
}
