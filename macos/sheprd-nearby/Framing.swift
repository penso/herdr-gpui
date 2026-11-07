import Foundation

/// How a message crosses the `pair` characteristic: a 2-byte big-endian
/// length, then the message, cut into chunks a write or notification
/// carries. Kept identical in the iOS app (`NearbyFraming`).
enum Framing {
    /// The largest message either side accepts.
    static let maxMessage = 4096

    static func chunks(_ message: Data, size: Int) -> [Data] {
        let framed = Data([UInt8(message.count >> 8), UInt8(message.count & 0xff)]) + message
        let step = max(20, size)
        return stride(from: 0, to: framed.count, by: step).map { framed.subdata(in: $0..<min($0 + step, framed.count)) }
    }
}

/// Collects one message's chunks.
struct Reassembly {
    enum Outcome { case partial, complete(Data), invalid }

    private var buffer = Data()

    mutating func add(_ chunk: Data) -> Outcome {
        buffer.append(chunk)
        guard buffer.count >= 2 else { return .partial }
        let length = Int(buffer[buffer.startIndex]) << 8 | Int(buffer[buffer.startIndex + 1])
        guard length <= Framing.maxMessage, buffer.count <= length + 2 else { return .invalid }
        return buffer.count == length + 2 ? .complete(buffer.dropFirst(2)) : .partial
    }
}
