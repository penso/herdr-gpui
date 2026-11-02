import CoreBluetooth
import Foundation
import HerdrRemoteCore
import Observation

/// Computers whose pairing dialog is open nearby, found over Bluetooth:
/// the Mac's `herdr-nearby` helper advertises only then. Scans while the
/// pair sheet is open or no computer is paired, so the Bluetooth prompt
/// comes when the person is pairing. Callbacks arrive on the main queue
/// (`queue: nil`); the preconcurrency conformances check that at run time.
@MainActor
@Observable
final class NearbyBluetooth: NSObject {
    private(set) var computers: [NearbyComputer] = []
    private var manager: CBCentralManager?
    private var wanted = false
    private var peripherals: [UUID: CBPeripheral] = [:]
    /// Peripherals whose `info` is being read, to list them.
    private var probing: Set<UUID> = []
    private var exchanges: [UUID: Exchange] = [:]

    static let service = CBUUID(string: "DD750B9A-9568-4FC3-B357-B7515AE46DED")
    static let info = CBUUID(string: "820A3154-F1B2-47C2-B8BE-D89E812464A2")
    static let pair = CBUUID(string: "8D5498FD-CFE2-4D2C-99E9-1FDBC2D83892")

    /// One pairing's link to a peripheral: the characteristic, and the
    /// answer awaited.
    private final class Exchange {
        var ready: CheckedContinuation<Void, Error>?
        var answer: CheckedContinuation<Data, Error>?
        var characteristic: CBCharacteristic?
        var reassembly = NearbyFraming.Reassembly()
    }

    enum Failure: Error, LocalizedError {
        case gone
        case refused(String)

        var errorDescription: String? {
            switch self {
            case .gone: "The computer went out of Bluetooth range."
            case let .refused(message): message
            }
        }
    }

    func start() {
        wanted = true
        if let manager {
            if manager.state == .poweredOn { scan() }
        } else {
            manager = CBCentralManager(delegate: self, queue: nil)
        }
    }

    func stop() {
        wanted = false
        manager?.stopScan()
        for id in probing { peripherals[id].map { manager?.cancelPeripheralConnection($0) } }
        probing = []
        computers = []
    }

    private func scan() {
        guard wanted else { return }
        manager?.scanForPeripherals(withServices: [Self.service])
    }

    /// The two pairing messages over this peripheral's `pair` characteristic.
    func io(for identifier: UUID) -> NearbyIO {
        NearbyIO(
            start: { key in try await self.request(identifier, ["type": "start", "public_key": key]) },
            reveal: { session, nonce in try await self.request(identifier, ["type": "reveal", "session": session, "nonce": nonce]) }
        )
    }

    private func request<Answer: Decodable>(_ identifier: UUID, _ body: [String: String]) async throws -> Answer {
        guard let manager, let peripheral = peripherals[identifier] else { throw Failure.gone }
        let exchange = exchanges[identifier] ?? Exchange()
        exchanges[identifier] = exchange
        if exchange.characteristic == nil {
            try await withCheckedThrowingContinuation { (ready: CheckedContinuation<Void, Error>) in
                exchange.ready = ready
                peripheral.delegate = self
                if peripheral.state == .connected {
                    peripheral.discoverServices([Self.service])
                } else {
                    manager.connect(peripheral)
                }
            }
        }
        guard let characteristic = exchange.characteristic else { throw Failure.gone }
        let message = try JSONEncoder().encode(body)
        let data = try await withCheckedThrowingContinuation { (answer: CheckedContinuation<Data, Error>) in
            exchange.answer = answer
            exchange.reassembly = NearbyFraming.Reassembly()
            for chunk in NearbyFraming.chunks(message, size: peripheral.maximumWriteValueLength(for: .withResponse)) {
                peripheral.writeValue(chunk, for: characteristic, type: .withResponse)
            }
        }
        if let refusal = try? JSONDecoder().decode([String: String].self, from: data), let error = refusal["error"] {
            throw Failure.refused(error)
        }
        return try JSONDecoder().decode(Answer.self, from: data)
    }

    private func fail(_ identifier: UUID, _ error: Error) {
        guard let exchange = exchanges.removeValue(forKey: identifier) else { return }
        exchange.ready?.resume(throwing: error)
        exchange.answer?.resume(throwing: error)
    }
}

extension NearbyBluetooth: @preconcurrency CBCentralManagerDelegate, @preconcurrency CBPeripheralDelegate {
    func centralManagerDidUpdateState(_ central: CBCentralManager) {
        if central.state == .poweredOn { scan() }
    }

    func centralManager(_ central: CBCentralManager, didDiscover peripheral: CBPeripheral, advertisementData: [String: Any], rssi: NSNumber) {
        let id = peripheral.identifier
        guard peripherals[id] == nil else { return }
        peripherals[id] = peripheral
        probing.insert(id)
        peripheral.delegate = self
        central.connect(peripheral)
    }

    func centralManager(_ central: CBCentralManager, didConnect peripheral: CBPeripheral) {
        peripheral.discoverServices([Self.service])
    }

    func centralManager(_ central: CBCentralManager, didFailToConnect peripheral: CBPeripheral, error: Error?) {
        drop(peripheral.identifier)
    }

    func centralManager(_ central: CBCentralManager, didDisconnectPeripheral peripheral: CBPeripheral, error: Error?) {
        // Listed computers stay until scanning stops; a dialog closed since
        // then answers the next pairing attempt with a refusal.
        fail(peripheral.identifier, Failure.gone)
        if probing.contains(peripheral.identifier) { drop(peripheral.identifier) }
    }

    private func drop(_ id: UUID) {
        peripherals[id] = nil
        probing.remove(id)
        computers.removeAll { $0.route == .bluetooth(identifier: id) }
    }

    func peripheral(_ peripheral: CBPeripheral, didDiscoverServices error: Error?) {
        guard let service = peripheral.services?.first(where: { $0.uuid == Self.service }) else { return }
        peripheral.discoverCharacteristics([Self.info, Self.pair], for: service)
    }

    func peripheral(_ peripheral: CBPeripheral, didDiscoverCharacteristicsFor service: CBService, error: Error?) {
        let id = peripheral.identifier
        for characteristic in service.characteristics ?? [] {
            if characteristic.uuid == Self.info, probing.contains(id) {
                peripheral.readValue(for: characteristic)
            } else if characteristic.uuid == Self.pair, let exchange = exchanges[id] {
                exchange.characteristic = characteristic
                peripheral.setNotifyValue(true, for: characteristic)
            }
        }
    }

    func peripheral(_ peripheral: CBPeripheral, didUpdateNotificationStateFor characteristic: CBCharacteristic, error: Error?) {
        guard let exchange = exchanges[peripheral.identifier] else { return }
        if let error {
            fail(peripheral.identifier, error)
        } else {
            exchange.ready?.resume()
            exchange.ready = nil
        }
    }

    func peripheral(_ peripheral: CBPeripheral, didUpdateValueFor characteristic: CBCharacteristic, error: Error?) {
        let id = peripheral.identifier
        let value = characteristic.value ?? Data()
        if characteristic.uuid == Self.info {
            probing.remove(id)
            if let fields = try? JSONSerialization.jsonObject(with: value) as? [String: Any],
               let name = fields["name"] as? String, let prefix = fields["fp"] as? String,
               let computer = NearbyComputer.fromTXT(["v": "1", "fp": prefix, "pairing": "1"], name: name, route: .bluetooth(identifier: id)) {
                computers.removeAll { $0.route == computer.route }
                computers.append(computer)
            }
            // Listed; the connection is opened again when pairing starts.
            manager?.cancelPeripheralConnection(peripheral)
            return
        }
        guard let exchange = exchanges[id] else { return }
        switch exchange.reassembly.add(value) {
        case let .complete(message):
            exchange.answer?.resume(returning: message)
            exchange.answer = nil
        case .invalid:
            fail(id, Failure.gone)
        case .partial:
            break
        }
    }
}
