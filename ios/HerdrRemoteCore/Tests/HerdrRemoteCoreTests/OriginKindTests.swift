import Testing
@testable import HerdrRemoteCore

struct OriginKindTests {
    @Test(arguments: [
        ("https://100.84.59.116:7443", OriginKind.tailscale),
        ("https://100.127.255.1:7443", .tailscale),
        ("https://100.128.0.1:7443", .other),
        ("https://[fd7a:115c:a1e0::d801:3b74]:7443", .tailscale),
        ("https://studio.tail1234.ts.net:7443", .tailscale),
        ("https://10.0.0.19:7443", .localNetwork),
        ("https://192.168.1.20:7443", .localNetwork),
        ("https://172.20.0.3:7443", .localNetwork),
        ("https://172.32.0.3:7443", .other),
        ("https://[fd07:b51a:cc66::1]:7443", .localNetwork),
        ("https://[fe80::1465:beca:6126:bd64]:7443", .localNetwork),
        ("https://m4max.local:7443", .localName),
        ("https://127.0.0.1:7443", .loopback),
        ("https://[::1]:7443", .loopback),
        ("https://localhost:7443", .loopback),
        ("https://8.8.8.8:7443", .other),
        ("not a url", .other),
    ])
    func namesTheAddress(origin: String, kind: OriginKind) {
        #expect(OriginKind(origin: origin) == kind)
    }
}
