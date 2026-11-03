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
    /// The windows that want scanning (`look`); it runs while any does.
    private var lookers: Set<ObjectIdentifier> = []
    private var peripherals: [UUID: CBPeripheral] = [:]
    /// Peripherals whose `info` is being read, to list them.
    private var probing: Set<UUID> = []
    private var exchanges: [UUID: Exchange] = [:]
    /// When each peripheral last advertised, to drop a computer whose
    /// pairing dialog closed.
    private var sightings = NearbySightings()
    private var expiry: Task<Void, Never>?

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

    /// Whether `window` wants scanning. One window closing its pair sheet
    /// leaves another that is still pairing scanning.
    func look(_ looking: Bool, for window: AnyObject) {
        let id = ObjectIdentifier(window)
        let was = !lookers.isEmpty
        if looking { lookers.insert(id) } else { lookers.remove(id) }
        switch (was, !lookers.isEmpty) {
        case (false, true): start()
        case (true, false): stop()
        default: break
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
        // Forget every peripheral, not only those being probed: a scan
        // ignores one it already knows, so a computer paired, then
        // forgotten, would never be listed again while the app runs.
        for (id, peripheral) in peripherals {
            fail(id, Failure.gone)
            manager?.cancelPeripheralConnection(peripheral)
        }
        peripherals = [:]
        probing = []
        computers = []
        sightings.removeAll()
        expiry?.cancel()
        expiry = nil
    }

    private func scan() {
        guard wanted else { return }
        // Every advertisement, not only the first: one that stops means the
        // computer's dialog closed. iOS coalesces them in the background,
        // where a listed computer then expires, as no card shows there.
        manager?.scanForPeripherals(withServices: [Self.service], options: [CBCentralManagerScanOptionAllowDuplicatesKey: true])
        guard expiry == nil else { return }
        expiry = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(2))
                self?.expireSilent()
            }
        }
    }

    /// Drops computers that stopped advertising, unless pairing with them.
    private func expireSilent() {
        for id in sightings.expire(at: Date(), keeping: Set(exchanges.keys)) {
            if probing.contains(id), let peripheral = peripherals[id] {
                manager?.cancelPeripheralConnection(peripheral)
            }
            drop(id)
        }
    }

    /// The two pairing messages over this peripheral's `pair` characteristic.
    /// The exchange ends with the reveal, or with the first failure: either
    /// way it is released, so the computer can age out of the list again.
    func io(for identifier: UUID) -> NearbyIO {
        NearbyIO(
            start: { key in
                try await self.releasing(identifier, after: false) {
                    try await self.request(identifier, ["type": "start", "public_key": key])
                }
            },
            reveal: { session, nonce in
                try await self.releasing(identifier, after: true) {
                    try await self.request(identifier, ["type": "reveal", "session": session, "nonce": nonce])
                }
            }
        )
    }

    /// Runs one step of an exchange, releasing it when the step fails, and
    /// after it when `after` (the last step).
    private func releasing<Answer>(_ identifier: UUID, after: Bool, _ step: () async throws -> Answer) async throws -> Answer {
        do {
            let answer = try await step()
            if after { release(identifier) }
            return answer
        } catch {
            release(identifier)
            throw error
        }
    }

    /// Ends an exchange: forgets it and closes the pairing connection.
    private func release(_ identifier: UUID) {
        fail(identifier, CancellationError())
        if let peripheral = peripherals[identifier] { manager?.cancelPeripheralConnection(peripheral) }
    }

    /// One message and its answer. Cancelling the pairing (closing the
    /// card) fails the exchange, which resumes whatever it was waiting on.
    private func request<Answer: Decodable>(_ identifier: UUID, _ body: [String: String]) async throws -> Answer {
        try await withTaskCancellationHandler {
            try await send(identifier, body)
        } onCancel: {
            Task { @MainActor in self.fail(identifier, CancellationError()) }
        }
    }

    private func send<Answer: Decodable>(_ identifier: UUID, _ body: [String: String]) async throws -> Answer {
        try Task.checkCancellation()
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
        _ = sightings.saw(id, at: Date())
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
        // Listed computers stay until they stop advertising (`expireSilent`);
        // a dialog closed since then answers a pairing attempt with a refusal.
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
