import Dispatch
import XCTest
@testable import SheprdRemoteCore

final class FirstCompletionTests: XCTestCase {
    private enum Failure: Error, Equatable {
        case first
        case second
    }

    private func outcome(_ start: (@escaping @Sendable (Error?) -> Void) -> Void) async -> Failure? {
        do {
            try await firstCompletion(start)
            return nil
        } catch {
            return error as? Failure
        }
    }

    func testOneCallAnswers() async {
        let success = await outcome { $0(nil) }
        XCTAssertNil(success)
        let failure = await outcome { $0(Failure.first) }
        XCTAssertEqual(failure, .first)
    }

    /// The TestFlight crash: `sendPing` reported the lost network twice.
    func testASecondFailureIsIgnored() async {
        let result = await outcome { done in
            done(Failure.first)
            done(Failure.second)
        }
        XCTAssertEqual(result, .first)
    }

    func testTheFirstCallDecidesTheAnswer() async {
        let success = await outcome { done in
            done(nil)
            done(Failure.second)
        }
        XCTAssertNil(success)
        let failure = await outcome { done in
            done(Failure.first)
            done(nil)
        }
        XCTAssertEqual(failure, .first)
    }

    func testACallAfterTheWaitEndedIsIgnored() async {
        let late = DispatchGroup()
        let result = await outcome { done in
            done(Failure.first)
            DispatchQueue.global().async(group: late) { done(Failure.second) }
        }
        XCTAssertEqual(result, .first)
        await withCheckedContinuation { finished in
            late.notify(queue: .global()) { finished.resume() }
        }
    }

    func testCallsFromManyThreadsResumeOnce() async {
        let result = await outcome { done in
            DispatchQueue.concurrentPerform(iterations: 64) { _ in done(Failure.first) }
        }
        XCTAssertEqual(result, .first)
    }
}
