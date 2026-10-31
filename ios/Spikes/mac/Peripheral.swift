// The Mac side of the Bluetooth spike: a GATT peripheral with the nearby
// service, an echo characteristic, and an L2CAP channel that receives
// 1 MiB and sends 1 MiB back. Built into HerdrBLESpike.app by run.sh, so
// macOS asks for Bluetooth permission for the bundle.

import CoreBluetooth
import Foundation

let service = CBUUID(string: "DD750B9A-9568-4FC3-B357-B7515AE46DED")
let infoID = CBUUID(string: "820A3154-F1B2-47C2-B8BE-D89E812464A2")
let echoID = CBUUID(string: "8D5498FD-CFE2-4D2C-99E9-1FDBC2D83892")
let total = 1 << 20

func log(_ text: String) {
    let stamp = ISO8601DateFormatter().string(from: Date())
    print("\(stamp) \(text)")
    fflush(stdout)
}

final class Peripheral: NSObject, CBPeripheralManagerDelegate, StreamDelegate {
    private var manager: CBPeripheralManager!
    private let echo = CBMutableCharacteristic(type: echoID, properties: [.write, .notify], value: nil, permissions: [.writeable])
    private let info = CBMutableCharacteristic(type: infoID, properties: [.read], value: nil, permissions: [.readable])
    private var psm: CBL2CAPPSM = 0
    private var channel: CBL2CAPChannel?
    private var received = 0
    private var sent = 0
    private var started: Date?

    func start() {
        manager = CBPeripheralManager(delegate: self, queue: nil)
    }

    func peripheralManagerDidUpdateState(_ peripheral: CBPeripheralManager) {
        log("state \(peripheral.state.rawValue) authorization \(CBPeripheralManager.authorization.rawValue)")
        guard peripheral.state == .poweredOn else { return }
        let gatt = CBMutableService(type: service, primary: true)
        gatt.characteristics = [info, echo]
        peripheral.add(gatt)
        peripheral.publishL2CAPChannel(withEncryption: false)
    }

    func peripheralManager(_ peripheral: CBPeripheralManager, didAdd service: CBService, error: Error?) {
        log("service added error=\(String(describing: error))")
        peripheral.startAdvertising([
            CBAdvertisementDataServiceUUIDsKey: [service.uuid],
            CBAdvertisementDataLocalNameKey: "Herdr spike",
        ])
    }

    func peripheralManagerDidStartAdvertising(_ peripheral: CBPeripheralManager, error: Error?) {
        log("advertising error=\(String(describing: error))")
    }

    func peripheralManager(_ peripheral: CBPeripheralManager, didPublishL2CAPChannel PSM: CBL2CAPPSM, error: Error?) {
        psm = PSM
        log("L2CAP published psm=\(PSM) error=\(String(describing: error))")
    }

    func peripheralManager(_ peripheral: CBPeripheralManager, didReceiveRead request: CBATTRequest) {
        let body = #"{"name":"\#(Host.current().localizedName ?? "Mac")","psm":\#(psm)}"#
        request.value = Data(body.utf8).dropFirst(request.offset)
        peripheral.respond(to: request, withResult: .success)
        log("read info from \(request.central.identifier)")
    }

    func peripheralManager(_ peripheral: CBPeripheralManager, didReceiveWrite requests: [CBATTRequest]) {
        for request in requests {
            if let value = request.value {
                peripheral.updateValue(value, for: echo, onSubscribedCentrals: [request.central])
            }
        }
        if let first = requests.first { peripheral.respond(to: first, withResult: .success) }
    }

    func peripheralManager(_ peripheral: CBPeripheralManager, central: CBCentral, didSubscribeTo characteristic: CBCharacteristic) {
        log("subscribed \(central.identifier) mtu=\(central.maximumUpdateValueLength)")
    }

    func peripheralManager(_ peripheral: CBPeripheralManager, didOpen channel: CBL2CAPChannel?, error: Error?) {
        log("L2CAP opened error=\(String(describing: error))")
        guard let channel else { return }
        self.channel = channel
        received = 0
        sent = 0
        started = nil
        for stream in [channel.inputStream as Stream, channel.outputStream as Stream] {
            stream.delegate = self
            stream.schedule(in: .main, forMode: .default)
            stream.open()
        }
    }

    func stream(_ stream: Stream, handle event: Stream.Event) {
        guard let channel else { return }
        if event.contains(.hasBytesAvailable), let input = stream as? InputStream {
            var buffer = [UInt8](repeating: 0, count: 16 * 1024)
            while input.hasBytesAvailable {
                let count = input.read(&buffer, maxLength: buffer.count)
                guard count > 0 else { break }
                if started == nil { started = Date() }
                received += count
            }
            if received >= total, let started {
                let seconds = Date().timeIntervalSince(started)
                log(String(format: "received %d bytes in %.2fs: %.1f KiB/s", received, seconds, Double(received) / 1024 / seconds))
                send(on: channel.outputStream)
            }
        }
        if event.contains(.hasSpaceAvailable), received >= total {
            send(on: channel.outputStream)
        }
        if event.contains(.endEncountered) || event.contains(.errorOccurred) {
            log("stream ended error=\(String(describing: stream.streamError))")
        }
    }

    private func send(on output: OutputStream) {
        let chunk = [UInt8](repeating: 0x5a, count: 4096)
        while sent < total, output.hasSpaceAvailable {
            let count = output.write(chunk, maxLength: min(chunk.count, total - sent))
            guard count > 0 else { break }
            sent += count
        }
        if sent >= total { log("sent \(sent) bytes back") }
    }
}

let peripheral = Peripheral()
peripheral.start()
log("started")
RunLoop.main.run()
