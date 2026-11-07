#if canImport(FoundationModels) && !os(watchOS)
import Foundation
import FoundationModels

/// The on-device model's reading of a sentence the rules could not place.
/// Its schema lists what exists, so it picks from the fleet rather than
/// writing names, and `VoiceDraft.adopting` still checks what it says.
public enum VoiceModel {
    /// More would crowd the model's small context, so the lists are cut.
    private static let listed = 16
    private static let unspecified = "unspecified"
    private static let instructions = """
    You turn one spoken sentence into a command for a coding app. Choose only from the listed values. \
    new_workspace starts an agent in a project or makes a worktree. ask_chat sends words to an existing chat. \
    stop_chat stops one. open_chat shows one. answer_chat allows or denies a chat's question. \
    needs_me asks what is waiting. Anything else is none. Copy the task word for word; never reword it. \
    Use unspecified when the sentence does not say.
    """

    public static var available: Bool {
        if case .available = SystemLanguageModel.default.availability { true } else { false }
    }

    /// Reads `text`, or nil when the model cannot: the rules' draft stands.
    public static func read(_ text: String, catalog: VoiceCatalog) async -> VoiceReading? {
        guard available, let schema = try? schema(catalog) else { return nil }
        let session = LanguageModelSession(instructions: instructions)
        guard let answer = try? await session.respond(to: text, schema: schema, options: GenerationOptions(temperature: 0)) else { return nil }
        let content = answer.content
        func value(_ name: String) -> String? {
            guard let value = try? content.value(String.self, forProperty: name), !value.isEmpty, value != unspecified else { return nil }
            return value
        }
        return VoiceReading(
            action: value("action") ?? VoiceIntent.none.rawValue,
            computer: value("computer"),
            project: value("project"),
            agent: value("agent"),
            chat: value("chat"),
            topic: value("topic"),
            task: value("task")
        )
    }

    private static func schema(_ catalog: VoiceCatalog) throws -> GenerationSchema {
        func choice(_ name: String, _ detail: String, _ values: [String]) -> DynamicGenerationSchema.Property {
            .init(name: name, description: detail, schema: DynamicGenerationSchema(name: "\(name)Choice", anyOf: Array(values.prefix(listed)) + [unspecified]))
        }
        func words(_ name: String, _ detail: String) -> DynamicGenerationSchema.Property {
            .init(name: name, description: detail, schema: DynamicGenerationSchema(type: String.self))
        }
        let root = DynamicGenerationSchema(name: "Command", properties: [
            .init(name: "action", description: "What to do", schema: DynamicGenerationSchema(name: "Action", anyOf: VoiceIntent.allCases.map(\.rawValue))),
            choice("computer", "The computer named, if any", catalog.computers.map(\.name)),
            choice("project", "The project or repository named, if any", catalog.projectNames),
            choice("agent", "The agent to start, if any", catalog.agents.map(\.id)),
            choice("chat", "The existing chat named, if any", catalog.chats.map(\.title)),
            words("topic", "What the worktree is about, in a few words, else empty"),
            words("task", "The exact words of what the agent is asked to do, copied from the sentence, else empty"),
        ])
        return try GenerationSchema(root: root, dependencies: [])
    }
}
#endif
