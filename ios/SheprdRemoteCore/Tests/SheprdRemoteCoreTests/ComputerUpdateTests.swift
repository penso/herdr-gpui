import Foundation
import Testing
@testable import SheprdRemoteCore

struct ComputerUpdateTests {
    private func home(_ json: String) throws -> Home {
        try JSONDecoder().decode(Home.self, from: Data(json.utf8))
    }

    @Test func aComputerThatDoesNotSayItsLevelNeedsAnUpdate() throws {
        let old = try home(#"{"computer":"a","fingerprint":"b","connected":true,"waiting":0,"workspaces":[]}"#)
        #expect(old.needsUpdate)
        #expect(old.version == nil)
    }

    @Test func aComputerAtTheRequiredLevelDoesNot() throws {
        let current = try home(#"{"api":\#(Home.requiredAPI),"version":"20261106.1","computer":"a","fingerprint":"b","connected":true,"waiting":0,"workspaces":[]}"#)
        #expect(!current.needsUpdate)
        #expect(current.version == "20261106.1")
    }

    @Test func theDaemonsAnswerDecodes() throws {
        let update = try JSONDecoder().decode(
            ComputerUpdate.self,
            from: Data(#"{"app":{"stage":"available","latest":"20261106.1"},"asked":true}"#.utf8)
        )
        #expect(update.app == .init(stage: .available, latest: "20261106.1"))
        #expect(update.working)
        #expect(!update.canAsk)
    }

    @Test func aClosedAppOrABuildThatCannotUpdateCannotBeAsked() {
        #expect(!ComputerUpdate(app: nil).canAsk)
        #expect(!ComputerUpdate(app: .init(stage: .disabled)).canAsk)
        #expect(!ComputerUpdate(app: .init(stage: .busy)).canAsk)
    }

    @Test func anIdleOrFailedUpdaterCanBeAsked() {
        for stage in [ComputerUpdate.Stage.available, .current, .failed, .idle] {
            #expect(ComputerUpdate(app: .init(stage: stage)).canAsk)
        }
    }
}
