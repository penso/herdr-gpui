import HerdrRemoteCore
import LocalAuthentication

/// Face ID, Touch ID, or the passcode, before an answer that lasts or an
/// app that is locked.
enum OwnerCheck {
    static func confirm(_ reason: String) async -> Bool {
        (try? await LAContext().evaluatePolicy(.deviceOwnerAuthentication, localizedReason: reason)) ?? false
    }

    /// "Always" answers outlive the question, so they ask first.
    static func allows(_ option: PermissionOption) async -> Bool {
        switch option.kind {
        case .allowAlways: await confirm("Always allow this without asking")
        case .rejectAlways: await confirm("Always deny this without asking")
        default: true
        }
    }
}
