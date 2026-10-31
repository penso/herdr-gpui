import CoreBluetooth
import Foundation
import HerdrRemoteCore
import os

private let log = Logger(subsystem: "so.pen.herdr.remote", category: "presence")

/// Bluetooth presence after pairing (`Presence`): keeps a connection to
/// each paired computer that has presence on and is in range, proves this
/// phone on the `presence` characteristic, and says whether it is near, so
/// the computer holds alerts while you are at it.
///
/// - A pending connection never times out, so iOS reconnects when the
///   computer comes back in range, and state restoration relaunches the
///   app in the background to do it.
/// - The peripheral does not say which computer it is, so the phone says
///   hello, by a daily hint rather than its device id, as each computer
///   with presence on until one answers. It signs only a challenge signed
///   by that computer's pinned CA, and the sealed addresses that answer a
///   proof open only with that computer's key.
/// - Signal strength is read while the app runs; in the background the
///   last state stands until the connection drops, which counts as away.
///
/// Callbacks arrive on the main queue (`queue: nil`); the preconcurrency
/// conformances check that at run time.
@MainActor
final class PresenceCentral: NSObject {
    static let presence = CBUUID(string: "6E3A4F21-8C0B-4D7E-9A55-2F1C0B7D4E93")
    static let restoreIdentifier = "so.pen.herdr.remote.presence"

    /// The paired computers, kept current by the app model.
    var computers: [Computer] = [] {
        didSet { apply() }
    }
    /// A computer sent its current addresses.
    var onAddresses: ((Fingerprint, [String]) -> Void)?
    /// iOS relaunched the app because a computer came back in range.
    var onWake: (() -> Void)?

    private var manager: CBCentralManager?
    private var links: [UUID: Link] = [:]
    private var ticker: Task<Void, Never>?

    /// One peripheral: where its proof stands and its nearness.
    private final class Link {
        let peripheral: CBPeripheral
        var characteristic: CBCharacteristic?
        var reassembly = NearbyFraming.Reassembly()
        /// Computers not yet tried with hello.
        var candidates: [Computer] = []
        var trying: Computer?
        /// A proof was sent for `trying` and awaits its answer.
        var challenged = false
        var proven: Computer?
        var nearness = Presence.Nearness()

        init(_ peripheral: CBPeripheral) { self.peripheral = peripheral }
    }

    private var wanted: [Computer] { computers.filter(\.presence) }

    /// Turns Bluetooth on or off for presence as computers opt in or out.
    private func apply() {
        if wanted.isEmpty {
            ticker?.cancel()
            ticker = nil
            if let manager {
                manager.stopScan()
                for link in links.values { manager.cancelPeripheralConnection(link.peripheral) }
            }
            links = [:]
            return
        }
        dropUnwanted()
        if manager == nil {
            manager = CBCentralManager(
                delegate: self,
                queue: nil,
                options: [CBCentralManagerOptionRestoreIdentifierKey: Self.restoreIdentifier]
            )
        } else if manager?.state == .poweredOn {
            scan()
        }
        if ticker == nil {
            ticker = Task { [weak self] in
                while !Task.isCancelled {
                    try? await Task.sleep(for: .seconds(10))
                    self?.readSignal()
                }
            }
        }
    }

    /// A computer whose presence was turned off loses its link here, or
    /// it would stay proven and keep holding alerts, and it is no longer
    /// tried on links still finding out which computer they are.
    private func dropUnwanted() {
        let devices = Set(wanted.map(\.device))
        for (id, link) in links {
            if let proven = link.proven, !devices.contains(proven.device) {
                manager?.cancelPeripheralConnection(link.peripheral)
                links[id] = nil
                continue
            }
            link.candidates.removeAll { !devices.contains($0.device) }
            // Its answer may still be on the way and would be read as the
            // next computer's: start over on a fresh connection, which
            // reconnects and tries only the computers still wanted.
            if let trying = link.trying, !devices.contains(trying.device) {
                link.trying = nil
                link.challenged = false
                manager?.cancelPeripheralConnection(link.peripheral)
            }
        }
    }

    private func scan() {
        guard let manager, !wanted.isEmpty else { return }
        manager.scanForPeripherals(withServices: [NearbyBluetooth.service])
    }

    private func readSignal() {
        for link in links.values where link.proven != nil && link.peripheral.state == .connected {
            link.peripheral.readRSSI()
        }
    }

    // MARK: Exchange

    private func helloNext(_ link: Link) {
        guard let next = link.candidates.first else {
            // No computer with presence on knows this phone here.
            link.trying = nil
            return
        }
        link.candidates.removeFirst()
        link.trying = next
        send(.hello(hint: Presence.hint(device: next.device, day: Presence.day(.now))), on: link)
    }

    private func received(_ message: Data, on link: Link) {
        guard let incoming = try? JSONDecoder().decode(Presence.Incoming.self, from: message) else { return }
        switch incoming {
        case let .challenge(nonce, certificate, signature):
            guard let computer = link.trying else { return }
            // Not the pinned computer, or a relay rewriting the challenge:
            // sign nothing, and try the next computer.
            guard Presence.challengeIsFrom(computer.fingerprint, device: computer.device, nonce: nonce, certificate: certificate, signature: signature) else {
                log.error("A presence challenge did not come from \(computer.name, privacy: .public)")
                helloNext(link)
                return
            }
            do {
                let key = try DeviceKey(kind: computer.keyKind, data: computer.keyData)
                let proof = DeviceKeyEncoding.signature(try key.signature(for: try Presence.message(fingerprint: computer.fingerprint, nonce: nonce)))
                link.challenged = true
                send(.prove(nonce: nonce, signature: proof), on: link)
            } catch {
                log.error("Could not sign presence: \(error.localizedDescription, privacy: .public)")
            }
        case let .proven(sealed):
            // Only as the answer to a proof this phone sent on this link.
            guard let computer = link.trying, link.challenged else { return }
            link.challenged = false
            if let sealed {
                // Only this computer's key opens it; a peripheral that is
                // another computer gets no say over these addresses.
                guard let key = try? NotifyKey.loadAll()[computer.fingerprint],
                      let origins = try? Presence.openAddresses(sealed, device: key.publicKey, agree: { try key.agree($0) })
                else { return }
                onAddresses?(computer.fingerprint, origins)
            }
            link.proven = computer
            link.trying = nil
            link.peripheral.readRSSI()
        case .error:
            helloNext(link)
        case .other:
            break
        }
    }

    private func send(_ frame: Presence.Outgoing, on link: Link) {
        guard let characteristic = link.characteristic, let body = try? JSONEncoder().encode(frame) else { return }
        let size = link.peripheral.maximumWriteValueLength(for: .withResponse)
        for chunk in NearbyFraming.chunks(body, size: size) {
            link.peripheral.writeValue(chunk, for: characteristic, type: .withResponse)
        }
    }

    private func attach(_ peripheral: CBPeripheral) -> Link {
        if let link = links[peripheral.identifier] { return link }
        let link = Link(peripheral)
        peripheral.delegate = self
        links[peripheral.identifier] = link
        return link
    }

    private func start(_ link: Link) {
        link.proven = nil
        link.trying = nil
        link.challenged = false
        // Nothing half-read from the last connection carries over.
        link.reassembly = NearbyFraming.Reassembly()
        link.nearness = Presence.Nearness()
        link.candidates = wanted
        link.peripheral.discoverServices([NearbyBluetooth.service])
    }
}

extension PresenceCentral: @preconcurrency CBCentralManagerDelegate {
    func centralManagerDidUpdateState(_ central: CBCentralManager) {
        if central.state == .poweredOn { scan() }
    }

    func centralManager(_ central: CBCentralManager, willRestoreState state: [String: Any]) {
        let restored = state[CBCentralManagerRestoredStatePeripheralsKey] as? [CBPeripheral] ?? []
        for peripheral in restored { _ = attach(peripheral) }
        onWake?()
    }

    func centralManager(_ central: CBCentralManager, didDiscover peripheral: CBPeripheral, advertisementData: [String: Any], rssi: NSNumber) {
        guard links[peripheral.identifier] == nil else { return }
        _ = attach(peripheral)
        central.connect(peripheral)
    }

    func centralManager(_ central: CBCentralManager, didConnect peripheral: CBPeripheral) {
        start(attach(peripheral))
    }

    func centralManager(_ central: CBCentralManager, didDisconnectPeripheral peripheral: CBPeripheral, error: Error?) {
        // Away until it is back; a pending connect waits for that.
        // One `dropUnwanted` let go stays gone.
        guard let link = links[peripheral.identifier] else { return }
        link.proven = nil
        link.characteristic = nil
        if !wanted.isEmpty { central.connect(peripheral) }
    }

    func centralManager(_ central: CBCentralManager, didFailToConnect peripheral: CBPeripheral, error: Error?) {
        links[peripheral.identifier] = nil
    }
}

extension PresenceCentral: @preconcurrency CBPeripheralDelegate {
    func peripheral(_ peripheral: CBPeripheral, didDiscoverServices error: Error?) {
        guard let service = peripheral.services?.first(where: { $0.uuid == NearbyBluetooth.service }) else { return }
        peripheral.discoverCharacteristics([Self.presence], for: service)
    }

    func peripheral(_ peripheral: CBPeripheral, didDiscoverCharacteristicsFor service: CBService, error: Error?) {
        guard let characteristic = service.characteristics?.first(where: { $0.uuid == Self.presence }) else {
            // A computer without presence on: let it go.
            manager?.cancelPeripheralConnection(peripheral)
            links[peripheral.identifier] = nil
            return
        }
        links[peripheral.identifier]?.characteristic = characteristic
        peripheral.setNotifyValue(true, for: characteristic)
    }

    func peripheral(_ peripheral: CBPeripheral, didUpdateNotificationStateFor characteristic: CBCharacteristic, error: Error?) {
        guard error == nil, characteristic.isNotifying, let link = links[peripheral.identifier] else { return }
        helloNext(link)
    }

    func peripheral(_ peripheral: CBPeripheral, didUpdateValueFor characteristic: CBCharacteristic, error: Error?) {
        guard let link = links[peripheral.identifier], let chunk = characteristic.value else { return }
        switch link.reassembly.add(chunk) {
        case let .complete(message):
            link.reassembly = NearbyFraming.Reassembly()
            received(message, on: link)
        case .partial: break
        case .invalid: link.reassembly = NearbyFraming.Reassembly()
        }
    }

    func peripheral(_ peripheral: CBPeripheral, didReadRSSI RSSI: NSNumber, error: Error?) {
        guard error == nil, let link = links[peripheral.identifier], link.proven != nil else { return }
        if let near = link.nearness.update(rssi: RSSI.intValue) {
            send(.near(near), on: link)
        }
    }
}
