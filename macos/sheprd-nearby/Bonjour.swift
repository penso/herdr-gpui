import Foundation

/// The network half of nearby pairing on macOS: registers
/// `_herdr-remote._tcp` through the system's DNS-SD (mDNSResponder), with
/// `pairing=1`, while the pairing dialog is open. The daemon's own mDNS
/// responder is not visible to mDNSResponder on the same Mac, so here the
/// helper, which runs exactly while the dialog is open, advertises instead.
@MainActor
final class Bonjour {
    private var service: NetService?

    func publish(name: String, fingerprint: String, port: Int) {
        stop()
        guard port > 0, port <= Int(UInt16.max) else { return }
        let service = NetService(domain: "local.", type: "_herdr-remote._tcp.", name: name, port: Int32(port))
        let record: [String: Data] = ["v": Data("1".utf8), "fp": Data(fingerprint.utf8), "pairing": Data("1".utf8)]
        service.setTXTRecord(NetService.data(fromTXTRecord: record))
        service.publish()
        self.service = service
    }

    func stop() {
        service?.stop()
        service = nil
    }
}
