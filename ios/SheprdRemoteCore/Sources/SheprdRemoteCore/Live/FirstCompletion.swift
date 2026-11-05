import Synchronization

/// Waits for a completion handler that may be called more than once, and
/// answers with its first call.
///
/// `URLSessionWebSocketTask.sendPing` is such a handler: when the network
/// drops it can report the failure twice, and resuming a continuation a
/// second time traps, so losing the network took the whole app down.
public func firstCompletion(_ start: (@escaping @Sendable (Error?) -> Void) -> Void) async throws {
    try await withCheckedThrowingContinuation { (done: CheckedContinuation<Void, Error>) in
        let once = Once(done)
        start { once.resume($0) }
    }
}

private final class Once: Sendable {
    private let pending: Mutex<CheckedContinuation<Void, Error>?>

    init(_ continuation: CheckedContinuation<Void, Error>) {
        pending = Mutex(continuation)
    }

    func resume(_ error: Error?) {
        guard let continuation = pending.withLock({ $0.take() }) else { return }
        if let error {
            continuation.resume(throwing: error)
        } else {
            continuation.resume()
        }
    }
}
