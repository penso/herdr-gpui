import CoreBluetooth
import Foundation
import Observation

/// The phone side of the Bluetooth spike: finds the Mac's service, reads
/// its info, times echo round trips, and moves 1 MiB each way over L2CAP.
/// State restoration relaunches the app when the Mac comes back in range;
/// those relaunches are kept in the log.
@MainActor
@Observable
final class Central: NSObject {
    private(set) var lines: [String] = []
    private var manager: CBCentralManager?
    private var peripheral: CBPeripheral?
    private var echo: CBCharacteristic?
    private var psm: CBL2CAPPSM?
    private var pings: [Date] = []
    private var roundTrips: [TimeInterval] = []
    private var channel: CBL2CAPChannel?
    private var sent = 0
    private var received = 0
    private var started: Date?
    private var reading = false

    static let restoreKey = "herdr-spike-central"
    private static let saved = "central-log"

    override init() {
        super.init()
        lines = UserDefaults.standard.stringArray(forKey: Self.saved) ?? []
    }

    func start() {
        guard manager == nil else { return }
        manager = CBCentralManager(
            delegate: self,
            queue: nil,
            options: [CBCentralManagerOptionRestoreIdentifierKey: Self.restoreKey]
        )
    }

    func clear() {
        lines = []
        UserDefaults.standard.removeObject(forKey: Self.saved)
    }

    fileprivate func log(_ text: String) {
        let stamp = Date().formatted(date: .omitted, time: .standard)
        lines.append("\(stamp) \(text)")
        lines = Array(lines.suffix(200))
        UserDefaults.standard.set(lines, forKey: Self.saved)
    }

    fileprivate func scan() {
        manager?.scanForPeripherals(withServices: [CBUUID(string: SpikeIDs.service)])
        log("scanning")
    }

    fileprivate func sendPing() {
        guard let peripheral, let echo else { return }
        pings.append(Date())
        peripheral.writeValue(Data("ping \(pings.count)".utf8), for: echo, type: .withResponse)
    }

    fileprivate func pump() {
        guard let output = channel?.outputStream else { return }
        let chunk = [UInt8](repeating: 0xa5, count: 4096)
        while sent < SpikeIDs.l2capBytes, output.hasSpaceAvailable {
            let count = output.write(chunk, maxLength: min(chunk.count, SpikeIDs.l2capBytes - sent))
            guard count > 0 else { break }
            if started == nil { started = Date() }
            sent += count
        }
        if sent >= SpikeIDs.l2capBytes, !reading, let started {
            reading = true
            log(String(format: "sent 1 MiB in %.2fs (queued)", Date().timeIntervalSince(started)))
            self.started = Date()
        }
    }

    fileprivate func drain() {
        guard let input = channel?.inputStream else { return }
        var buffer = [UInt8](repeating: 0, count: 16 * 1024)
        while input.hasBytesAvailable {
            let count = input.read(&buffer, maxLength: buffer.count)
            guard count > 0 else { break }
            received += count
        }
        if received >= SpikeIDs.l2capBytes, let started {
            let seconds = Date().timeIntervalSince(started)
            log(String(format: "received 1 MiB in %.2fs: %.1f KiB/s", seconds, Double(received) / 1024 / seconds))
            self.started = nil
        }
    }
}

// Callbacks arrive on the main queue (`queue: nil`); the preconcurrency
// conformances check that at run time, as Core Bluetooth has no Sendable
// annotations.
extension Central: @preconcurrency CBCentralManagerDelegate, @preconcurrency CBPeripheralDelegate, @preconcurrency StreamDelegate {
    func centralManagerDidUpdateState(_ central: CBCentralManager) {
        let state = central.state
        log("state \(state.rawValue)")
        if state == .poweredOn { scan() }
    }

    func centralManager(_ central: CBCentralManager, willRestoreState dict: [String: Any]) {
        let restored = (dict[CBCentralManagerRestoredStatePeripheralsKey] as? [CBPeripheral]) ?? []
        log("RESTORED by the system with \(restored.count) peripheral(s)")
        if let first = restored.first {
            peripheral = first
            first.delegate = self
        }
    }

    func centralManager(
        _ central: CBCentralManager,
        didDiscover peripheral: CBPeripheral,
        advertisementData: [String: Any],
        rssi RSSI: NSNumber
    ) {
        log("found \(peripheral.name ?? "?") rssi \(RSSI)")
        central.stopScan()
        self.peripheral = peripheral
        peripheral.delegate = self
        central.connect(peripheral)
    }

    func centralManager(_ central: CBCentralManager, didConnect peripheral: CBPeripheral) {
        log("connected, mtu \(peripheral.maximumWriteValueLength(for: .withResponse))")
        peripheral.discoverServices([CBUUID(string: SpikeIDs.service)])
    }

    func centralManager(_ central: CBCentralManager, didDisconnectPeripheral peripheral: CBPeripheral, error: Error?) {
        log("disconnected \(error?.localizedDescription ?? "")")
        // Reconnects whenever the Mac is in range again, even in the
        // background; state restoration relaunches the app for it.
        central.connect(peripheral)
    }

    func peripheral(_ peripheral: CBPeripheral, didDiscoverServices error: Error?) {
        for service in peripheral.services ?? [] {
            peripheral.discoverCharacteristics([CBUUID(string: SpikeIDs.info), CBUUID(string: SpikeIDs.echo)], for: service)
        }
    }

    func peripheral(_ peripheral: CBPeripheral, didDiscoverCharacteristicsFor service: CBService, error: Error?) {
        for characteristic in service.characteristics ?? [] {
            if characteristic.uuid == CBUUID(string: SpikeIDs.info) {
                peripheral.readValue(for: characteristic)
            } else if characteristic.uuid == CBUUID(string: SpikeIDs.echo) {
                echo = characteristic
                peripheral.setNotifyValue(true, for: characteristic)
            }
        }
    }

    func peripheral(_ peripheral: CBPeripheral, didUpdateNotificationStateFor characteristic: CBCharacteristic, error: Error?) {
        sendPing()
    }

    func peripheral(_ peripheral: CBPeripheral, didUpdateValueFor characteristic: CBCharacteristic, error: Error?) {
        let value = characteristic.value ?? Data()
        let isInfo = characteristic.uuid.uuidString == SpikeIDs.info
        if isInfo {
            log("info \(String(decoding: value, as: UTF8.self))")
            struct Info: Decodable { let psm: UInt16 }
            psm = (try? JSONDecoder().decode(Info.self, from: value))?.psm
            return
        }
        guard let sentAt = pings.last else { return }
        roundTrips.append(Date().timeIntervalSince(sentAt))
        if roundTrips.count < 20 {
            sendPing()
        } else {
            let sorted = roundTrips.sorted()
            log(String(format: "echo 20x: median %.0f ms, worst %.0f ms", sorted[10] * 1000, sorted[19] * 1000))
            if let psm { self.peripheral?.openL2CAPChannel(psm) }
        }
    }

    func peripheral(_ peripheral: CBPeripheral, didOpen channel: CBL2CAPChannel?, error: Error?) {
        log("L2CAP open \(error?.localizedDescription ?? "ok")")
        guard let channel else { return }
        self.channel = channel
        sent = 0
        received = 0
        for stream in [channel.inputStream as Stream, channel.outputStream as Stream] {
            stream.delegate = self
            stream.schedule(in: .main, forMode: .default)
            stream.open()
        }
    }

    func stream(_ aStream: Stream, handle eventCode: Stream.Event) {
        if eventCode.contains(.hasSpaceAvailable) { pump() }
        if eventCode.contains(.hasBytesAvailable) { drain() }
        if eventCode.contains(.errorOccurred) { log("stream error \(aStream.streamError?.localizedDescription ?? "")") }
    }
}
