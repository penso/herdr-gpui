// herdr-nearby: the Mac's Bluetooth side of nearby pairing and presence
// (docs/ios-app-plan.md). Herdr UI runs it while the pairing dialog is
// open, and while remote control runs with `[remote] presence` on, and
// talks to it in newline-delimited JSON on stdio. It advertises the
// nearby service and carries `pair` and `presence` frames between phones
// and Herdr UI, which relays them to the daemon. It holds no secrets and
// decides nothing: the code exchange and the presence proof run in the
// daemon.
//
// It also registers the `_herdr-remote._tcp` Bonjour service with
// `pairing=1` through the system's DNS-SD (Bonjour.swift) while it
// advertises for pairing.
// `--no-bluetooth` leaves Bluetooth alone: for Macs without it, and for
// tests that must not ask for the permission.
//
// From Herdr UI:  {"type":"advertise","name":"…","fp":"16 hex","port":"7443"}
//                 {"type":"advertise_presence","on":true|false}
//                 {"type":"frame","central":"UUID","data":"base64"}
//                 {"type":"presence_frame","central":"UUID","data":"base64"}
//                 {"type":"stop"}   (pairing's advertisement only)
// To Herdr UI:    {"type":"state","state":"powered_on|powered_off|unauthorized|unsupported|resetting|unknown"}
//                 {"type":"frame","central":"UUID","data":"base64"}
//                 {"type":"presence_frame","central":"UUID","data":"base64"}
//                 {"type":"disconnected","central":"UUID"}
//                 {"type":"idle","seconds":N}   (every 30 s while presence is on)
//                 {"type":"error","message":"…"}

import CoreBluetooth
import CoreGraphics
import Foundation

enum IDs {
    static let service = "DD750B9A-9568-4FC3-B357-B7515AE46DED"
    /// Read: `{"v":1,"name":…,"fp":…}`.
    static let info = "820A3154-F1B2-47C2-B8BE-D89E812464A2"
    /// Write and notify: framed JSON messages.
    static let pair = "8D5498FD-CFE2-4D2C-99E9-1FDBC2D83892"
    /// Write and notify: paired phones' presence, framed like `pair`.
    static let presence = "6E3A4F21-8C0B-4D7E-9A55-2F1C0B7D4E93"
}

func emit(_ message: [String: Any]) {
    guard let data = try? JSONSerialization.data(withJSONObject: message) else { return }
    FileHandle.standardOutput.write(data + Data("\n".utf8))
}

/// Peripheral callbacks arrive on the main queue (`queue: nil`); the
/// preconcurrency conformance checks that at run time.
@MainActor
final class Nearby: NSObject, @preconcurrency CBPeripheralManagerDelegate {
    private var manager: CBPeripheralManager!
    private let pair = CBMutableCharacteristic(type: CBUUID(string: IDs.pair), properties: [.write, .notify], value: nil, permissions: [.writeable])
    private let presence = CBMutableCharacteristic(type: CBUUID(string: IDs.presence), properties: [.write, .notify], value: nil, permissions: [.writeable])
    private let info = CBMutableCharacteristic(type: CBUUID(string: IDs.info), properties: [.read], value: nil, permissions: [.readable])
    /// The pairing advertisement, while the dialog shows an offer.
    private var advertised: (name: String, fingerprint: String)?
    /// Whether paired phones may report their presence.
    private var presenceOn = false
    private var serviceAdded = false
    private var incoming: [Key: Reassembly] = [:]
    private var centrals: [UUID: CBCentral] = [:]
    /// Notifications waiting for the transmit queue to drain.
    private var pending: [(Data, CBCentral, CBMutableCharacteristic)] = []
    private var idleTimer: Timer?

    /// A phone's partial message on one characteristic.
    private struct Key: Hashable {
        let central: UUID
        let presence: Bool
    }

    func start() {
        manager = CBPeripheralManager(delegate: self, queue: nil)
    }

    func advertise(name: String, fingerprint: String) {
        advertised = (name, fingerprint)
        apply()
    }

    /// Stops pairing's advertisement; presence's goes on.
    func stop() {
        advertised = nil
        apply()
    }

    func setPresence(_ on: Bool) {
        presenceOn = on
        idleTimer?.invalidate()
        idleTimer = nil
        if on {
            Self.reportIdleNow()
            idleTimer = Timer.scheduledTimer(withTimeInterval: 30, repeats: true) { _ in
                MainActor.assumeIsolated { Self.reportIdleNow() }
            }
        }
        apply()
    }

    /// Seconds since the last keyboard, mouse, or trackpad input anywhere
    /// in the login session.
    private static func reportIdleNow() {
        let any = CGEventType(rawValue: UInt32.max) ?? .null
        let seconds = CGEventSource.secondsSinceLastEventType(.combinedSessionState, eventType: any)
        emit(["type": "idle", "seconds": Int(max(0, seconds))])
    }

    /// Sends an answer to a phone, chunked to what it can take.
    func send(_ message: Data, to central: UUID, presence: Bool) {
        guard let target = centrals[central] else { return }
        let characteristic = presence ? self.presence : pair
        for chunk in Framing.chunks(message, size: target.maximumUpdateValueLength) {
            pending.append((chunk, target, characteristic))
        }
        flush()
    }

    private func flush() {
        while let (chunk, central, characteristic) = pending.first {
            guard manager.updateValue(chunk, for: characteristic, onSubscribedCentrals: [central]) else { return }
            pending.removeFirst()
        }
    }

    private func apply() {
        guard manager != nil, manager.state == .poweredOn else { return }
        if !serviceAdded {
            let service = CBMutableService(type: CBUUID(string: IDs.service), primary: true)
            service.characteristics = [info, pair, presence]
            manager.add(service)
            serviceAdded = true
        }
        let wanted = advertised != nil || presenceOn
        if wanted, !manager.isAdvertising {
            // Only the service UUID, which iOS needs to find it in the
            // background; the name stays in `info`, and only while pairing.
            manager.startAdvertising([CBAdvertisementDataServiceUUIDsKey: [CBUUID(string: IDs.service)]])
        } else if !wanted, manager.isAdvertising {
            manager.stopAdvertising()
        }
    }

    func peripheralManagerDidUpdateState(_ peripheral: CBPeripheralManager) {
        let names: [CBManagerState: String] = [
            .poweredOn: "powered_on", .poweredOff: "powered_off", .unauthorized: "unauthorized",
            .unsupported: "unsupported", .resetting: "resetting",
        ]
        emit(["type": "state", "state": names[peripheral.state] ?? "unknown"])
        apply()
    }

    func peripheralManager(_ peripheral: CBPeripheralManager, didAdd service: CBService, error: Error?) {
        if let error { emit(["type": "error", "message": "service: \(error.localizedDescription)"]) }
    }

    func peripheralManagerDidStartAdvertising(_ peripheral: CBPeripheralManager, error: Error?) {
        if let error { emit(["type": "error", "message": "advertising: \(error.localizedDescription)"]) }
    }

    func peripheralManager(_ peripheral: CBPeripheralManager, didReceiveRead request: CBATTRequest) {
        let fields: [String: Any]
        if let advertised {
            fields = ["v": 1, "name": advertised.name, "fp": advertised.fingerprint]
        } else if presenceOn {
            // No name or identity unauthenticated outside pairing.
            fields = ["v": 1]
        } else {
            peripheral.respond(to: request, withResult: .readNotPermitted)
            return
        }
        let body = (try? JSONSerialization.data(withJSONObject: fields)) ?? Data()
        guard request.offset <= body.count else {
            peripheral.respond(to: request, withResult: .invalidOffset)
            return
        }
        request.value = body.subdata(in: request.offset..<body.count)
        peripheral.respond(to: request, withResult: .success)
    }

    func peripheralManager(_ peripheral: CBPeripheralManager, didReceiveWrite requests: [CBATTRequest]) {
        let allowed = requests.allSatisfy { request in
            request.characteristic.uuid == presence.uuid ? presenceOn : advertised != nil
        }
        guard allowed else {
            if let first = requests.first { peripheral.respond(to: first, withResult: .writeNotPermitted) }
            return
        }
        for request in requests {
            let central = request.central.identifier
            let isPresence = request.characteristic.uuid == presence.uuid
            let key = Key(central: central, presence: isPresence)
            centrals[central] = request.central
            var assembly = incoming[key] ?? Reassembly()
            switch assembly.add(request.value ?? Data()) {
            case let .complete(message):
                incoming[key] = nil
                emit(["type": isPresence ? "presence_frame" : "frame", "central": central.uuidString, "data": message.base64EncodedString()])
            case .partial:
                incoming[key] = assembly
            case .invalid:
                incoming[key] = nil
            }
        }
        if let first = requests.first { peripheral.respond(to: first, withResult: .success) }
    }

    func peripheralManagerIsReady(toUpdateSubscribers peripheral: CBPeripheralManager) {
        flush()
    }

    func peripheralManager(_ peripheral: CBPeripheralManager, central: CBCentral, didUnsubscribeFrom characteristic: CBCharacteristic) {
        let isPresence = characteristic.uuid == presence.uuid
        incoming[Key(central: central.identifier, presence: isPresence)] = nil
        pending.removeAll { $0.1.identifier == central.identifier && $0.2.uuid == characteristic.uuid }
        if isPresence {
            emit(["type": "disconnected", "central": central.identifier.uuidString])
        }
    }
}

let bluetooth = !CommandLine.arguments.contains("--no-bluetooth")
let nearby = Nearby()
let bonjour = Bonjour()
if bluetooth { nearby.start() }

// Commands from Herdr UI, one JSON object per line, read off the main
// thread and applied on it. End of input means Herdr UI went away.
Thread.detachNewThread {
    while let line = readLine() {
        guard let object = try? JSONSerialization.jsonObject(with: Data(line.utf8)) as? [String: Any] else { continue }
        let type = object["type"] as? String, name = object["name"] as? String, fingerprint = object["fp"] as? String
        let port = (object["port"] as? String).flatMap { Int($0) } ?? 0
        let central = (object["central"] as? String).flatMap(UUID.init(uuidString:))
        let data = (object["data"] as? String).flatMap { Data(base64Encoded: $0) }
        let on = object["on"] as? Bool ?? false
        DispatchQueue.main.async {
            MainActor.assumeIsolated {
                switch type {
                case "advertise":
                    bonjour.publish(name: name ?? "Mac", fingerprint: fingerprint ?? "", port: port)
                    if bluetooth { nearby.advertise(name: name ?? "Mac", fingerprint: fingerprint ?? "") }
                case "advertise_presence":
                    if bluetooth { nearby.setPresence(on) }
                case "stop":
                    bonjour.stop()
                    if bluetooth { nearby.stop() }
                case "frame": if let central, let data { nearby.send(data, to: central, presence: false) }
                case "presence_frame": if let central, let data { nearby.send(data, to: central, presence: true) }
                default: break
                }
            }
        }
    }
    exit(0)
}
RunLoop.main.run()
