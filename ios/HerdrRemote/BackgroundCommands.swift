import Foundation
import HerdrRemoteCore

/// Sends a chat command while the app is not in front: from a
/// notification's action or a Live Activity's button. Live links are
/// closed then, so it signs in afresh at whichever of the computer's
/// addresses answers first.
enum BackgroundCommands {
    enum Failure: Error, LocalizedError {
        case unknownComputer
        case rejected(String)
        case notWaiting

        var errorDescription: String? {
            switch self {
            case .unknownComputer: "This phone is not paired with that computer."
            case let .rejected(reason): reason
            case .notWaiting: "That question is no longer waiting on this chat."
            }
        }
    }

    /// `computer` is the CA fingerprint of the computer whose key opened
    /// the notification.
    static func send(_ command: ChatCommand, computer fingerprint: String, pane: String) async throws {
        let session = try await signIn(fingerprint)
        if let rejected = try await session.client.command(command, pane: pane, bearer: session.token) {
            throw Failure.rejected(rejected)
        }
    }

    /// An answer from a Live Activity's button. Any paired computer can
    /// start an activity naming another computer's chat, so the answer goes
    /// only when that chat still waits on the question the activity showed.
    static func answerFromActivity(
        computer fingerprint: String,
        pane: String,
        permission: PermissionId,
        option: ChatActivityState.Option,
        question: String
    ) async throws {
        let session = try await signIn(fingerprint)
        let chat = try await session.client.chat(pane, bearer: session.token)
        guard ActivityAnswer.matches(question: question, permission: permission, option: option, in: chat.transcript) else {
            throw Failure.notWaiting
        }
        let command = ChatCommand.answer(permission: permission, option: option.id)
        if let rejected = try await session.client.command(command, pane: pane, bearer: session.token) {
            throw Failure.rejected(rejected)
        }
    }

    /// A prompt with pictures, from the share extension: each picture is
    /// uploaded first, then the prompt names them.
    static func sendPrompt(_ text: String, pictures: [Data], computer fingerprint: String, pane: String) async throws {
        let session = try await signIn(fingerprint)
        var ids: [String] = []
        for picture in pictures {
            ids.append(try await session.client.upload(picture, pane: pane, bearer: session.token))
        }
        let command = ChatCommand.prompt(text: text, attachments: ids)
        if let rejected = try await session.client.command(command, pane: pane, bearer: session.token) {
            throw Failure.rejected(rejected)
        }
    }

    private static func signIn(_ fingerprint: String) async throws -> (client: RemoteClient, token: String) {
        let computers = try ComputerStore.load()
        guard let wanted = Fingerprint(fingerprint),
              let computer = computers.first(where: { $0.fingerprint == wanted })
        else { throw Failure.unknownComputer }
        return try await Sessions().session(for: computer)
    }
}
