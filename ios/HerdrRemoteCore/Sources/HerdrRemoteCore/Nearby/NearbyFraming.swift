import Foundation

/// How a message crosses the Bluetooth `pair` characteristic: a 2-byte
/// big-endian length, then the message, cut into chunks one write or
/// notification carries. The Mac helper (`macos/herdr-nearby/Framing.swift`)
/// does the same.
public enum NearbyFraming {
    /// The largest message either side accepts.
    public static let maxMessage = 4096

    public static func chunks(_ message: Data, size: Int) -> [Data] {
        let framed = Data([UInt8(message.count >> 8), UInt8(message.count & 0xff)]) + message
        let step = max(20, size)
        return stride(from: 0, to: framed.count, by: step).map { framed.subdata(in: $0..<min($0 + step, framed.count)) }
    }

    /// Collects one message's chunks.
    public struct Reassembly: Sendable {
        public enum Outcome: Equatable, Sendable { case partial, complete(Data), invalid }

        private var buffer = Data()

        public init() {}

        public mutating func add(_ chunk: Data) -> Outcome {
            buffer.append(chunk)
            guard buffer.count >= 2 else { return .partial }
            let length = Int(buffer[buffer.startIndex]) << 8 | Int(buffer[buffer.startIndex + 1])
            guard length <= NearbyFraming.maxMessage, buffer.count <= length + 2 else { return .invalid }
            return buffer.count == length + 2 ? .complete(Data(buffer.dropFirst(2))) : .partial
        }
    }
}
