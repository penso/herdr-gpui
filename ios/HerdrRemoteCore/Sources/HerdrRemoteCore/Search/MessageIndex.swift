import CoreData
import Foundation

/// The messages of chats this phone has cached, in Core Data, so search
/// finds them while their computer is offline or runs a daemon that cannot
/// search. It keeps more of a chat than the display cache does: every user
/// and agent message it saw, up to `perChat` per chat and `kept` in all,
/// dropping the least recently seen first. It is a cache: a store that
/// does not open is deleted and started again.
public actor MessageIndex {
    /// Messages kept per chat, newest by entry.
    public static let perChat = 2_000
    /// Messages kept in all.
    public static let kept = 50_000

    /// A message that matched, with the words around the match.
    public struct Match: Hashable, Sendable {
        public var device: String
        public var hit: SearchHit
    }

    /// Where the store lives; nil keeps it in memory.
    public nonisolated let url: URL?
    private let container: NSPersistentContainer
    private var context: NSManagedObjectContext?

    public init(url: URL?) {
        self.url = url
        container = NSPersistentContainer(name: "Messages", managedObjectModel: Self.model())
        let description = NSPersistentStoreDescription(url: url ?? URL(filePath: "/dev/null"))
        description.shouldAddStoreAsynchronously = false
        #if os(iOS)
        // Readable only while the phone is unlocked, like the cache.
        description.setOption(FileProtectionType.complete as NSObject, forKey: NSPersistentStoreFileProtectionKey)
        #endif
        container.persistentStoreDescriptions = [description]
    }

    /// Records the user and agent messages of `transcript`, the chat
    /// `pane` on `device`. Messages already recorded keep their place;
    /// older ones the transcript no longer carries stay.
    public func save(_ transcript: Transcript, device: String, pane: String, now: Date = .now) async throws {
        let messages = transcript.entries.suffix(Self.perChat).compactMap(Self.message)
        guard !messages.isEmpty else { return }
        let context = try open()
        try await context.perform {
            let request = NSFetchRequest<NSManagedObject>(entityName: Self.entity)
            request.predicate = NSPredicate(format: "device == %@ AND pane == %@", device, pane)
            var known: [Int64: NSManagedObject] = [:]
            for object in try context.fetch(request) {
                known[object.value(forKey: "entry") as? Int64 ?? -1] = object
            }
            for (entry, author, text) in messages {
                let id = Int64(bitPattern: entry.value)
                if let object = known[id] {
                    guard object.value(forKey: "text") as? String != text else { continue }
                    object.setValue(text, forKey: "text")
                    object.setValue(MessageText.fold(text), forKey: "folded")
                    object.setValue(now, forKey: "seen")
                    continue
                }
                let object = NSEntityDescription.insertNewObject(forEntityName: Self.entity, into: context)
                object.setValue(device, forKey: "device")
                object.setValue(pane, forKey: "pane")
                object.setValue(id, forKey: "entry")
                object.setValue(author.rawValue, forKey: "author")
                object.setValue(text, forKey: "text")
                object.setValue(MessageText.fold(text), forKey: "folded")
                object.setValue(now, forKey: "seen")
                known[id] = object
            }
            if known.count > Self.perChat {
                for (_, object) in known.sorted(by: { $0.key < $1.key }).prefix(known.count - Self.perChat) {
                    context.delete(object)
                }
            }
            try context.save()
            try Self.trim(context, to: Self.kept)
        }
    }

    /// Messages on `devices` containing `query`, the most recently seen
    /// chats first, at most `perChat` from one chat and `limit` in all.
    public func search(_ query: String, devices: Set<String>, limit: Int = 60, perChat: Int = 3) async throws -> [Match] {
        guard let query = MessageText.query(query), !devices.isEmpty else { return [] }
        let context = try open()
        return try await context.perform {
            let request = NSFetchRequest<NSManagedObject>(entityName: Self.entity)
            request.predicate = NSPredicate(
                format: "device IN %@ AND folded CONTAINS %@",
                Array(devices), MessageText.fold(query)
            )
            request.sortDescriptors = [
                NSSortDescriptor(key: "seen", ascending: false),
                NSSortDescriptor(key: "entry", ascending: false),
            ]
            request.fetchLimit = limit * perChat * 4
            var counts: [String: Int] = [:]
            var matches: [Match] = []
            for object in try context.fetch(request) {
                guard let device = object.value(forKey: "device") as? String,
                      let pane = object.value(forKey: "pane") as? String,
                      let entry = object.value(forKey: "entry") as? Int64,
                      let author = (object.value(forKey: "author") as? String).flatMap(Author.init(rawValue:)),
                      let text = object.value(forKey: "text") as? String
                else { continue }
                let chat = "\(device)/\(pane)"
                guard counts[chat, default: 0] < perChat else { continue }
                counts[chat, default: 0] += 1
                matches.append(Match(device: device, hit: SearchHit(
                    pane: pane,
                    entry: EntryId(UInt64(bitPattern: entry)),
                    author: author,
                    snippet: MessageText.snippet(text, around: query)
                )))
                if matches.count == limit { break }
            }
            return matches
        }
    }

    /// Forgets one computer's messages, as unpairing does, or all of them.
    public func forget(device: String? = nil) async throws {
        let context = try open()
        try await context.perform {
            let request = NSFetchRequest<NSFetchRequestResult>(entityName: Self.entity)
            if let device { request.predicate = NSPredicate(format: "device == %@", device) }
            let delete = NSBatchDeleteRequest(fetchRequest: request)
            delete.resultType = .resultTypeObjectIDs
            let result = try context.execute(delete) as? NSBatchDeleteResult
            let deleted = result?.result as? [NSManagedObjectID] ?? []
            NSManagedObjectContext.mergeChanges(fromRemoteContextSave: [NSDeletedObjectsKey: deleted], into: [context])
        }
    }

    /// Bytes on disk, for Settings.
    public nonisolated func size() -> Int {
        guard let url else { return 0 }
        return ["", "-wal", "-shm"].reduce(0) { total, suffix in
            let file = URL(filePath: url.path + suffix)
            return total + ((try? file.resourceValues(forKeys: [.fileSizeKey]).fileSize) ?? 0)
        }
    }

    // MARK: Store

    private static let entity = "Message"

    /// The context, opening the store on first use; one that does not
    /// open, such as after a model change, is deleted and made again.
    private func open() throws -> NSManagedObjectContext {
        if let context { return context }
        if let url {
            try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        }
        if (try? load()) == nil, let url {
            try container.persistentStoreCoordinator.destroyPersistentStore(at: url, type: .sqlite)
            try load()
        }
        let context = container.newBackgroundContext()
        context.undoManager = nil
        self.context = context
        return context
    }

    private func load() throws {
        let failure = Failure()
        container.loadPersistentStores { _, error in failure.error = error }
        if let error = failure.error { throw error }
    }

    /// Carries the error out of the synchronous load callback.
    private final class Failure: @unchecked Sendable {
        var error: Error?
    }

    private static func message(_ entry: Entry) -> (EntryId, Author, String)? {
        switch entry.kind {
        case let .user(text, _): (entry.id, .user, text)
        case let .agent(text): (entry.id, .agent, text)
        default: nil
        }
    }

    /// Drops the least recently seen messages beyond `limit`.
    private static func trim(_ context: NSManagedObjectContext, to limit: Int) throws {
        let count = try context.count(for: NSFetchRequest<NSFetchRequestResult>(entityName: entity))
        guard count > limit else { return }
        let oldest = NSFetchRequest<NSManagedObject>(entityName: entity)
        oldest.sortDescriptors = [NSSortDescriptor(key: "seen", ascending: true)]
        oldest.fetchLimit = count - limit
        for object in try context.fetch(oldest) { context.delete(object) }
        try context.save()
    }

    /// One entity, `Message`, built in code so the model needs no
    /// `.xcdatamodeld` in either the app or the package.
    private static func model() -> NSManagedObjectModel {
        func attribute(_ name: String, _ type: NSAttributeType) -> NSAttributeDescription {
            let attribute = NSAttributeDescription()
            attribute.name = name
            attribute.attributeType = type
            attribute.isOptional = false
            return attribute
        }
        let message = NSEntityDescription()
        message.name = entity
        message.managedObjectClassName = NSStringFromClass(NSManagedObject.self)
        let device = attribute("device", .stringAttributeType)
        let pane = attribute("pane", .stringAttributeType)
        let seen = attribute("seen", .dateAttributeType)
        message.properties = [
            device, pane,
            attribute("entry", .integer64AttributeType),
            attribute("author", .stringAttributeType),
            attribute("text", .stringAttributeType),
            attribute("folded", .stringAttributeType),
            seen,
        ]
        message.indexes = [
            NSFetchIndexDescription(name: "byChat", elements: [
                NSFetchIndexElementDescription(property: device, collationType: .binary),
                NSFetchIndexElementDescription(property: pane, collationType: .binary),
            ]),
            NSFetchIndexDescription(name: "bySeen", elements: [
                NSFetchIndexElementDescription(property: seen, collationType: .binary),
            ]),
        ]
        let model = NSManagedObjectModel()
        model.entities = [message]
        return model
    }
}
