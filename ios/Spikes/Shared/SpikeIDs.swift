import Foundation

/// Identifiers the phone spikes and the Mac peripheral share. The service
/// and characteristic UUIDs are the ones nearby pairing will use.
enum SpikeIDs {
    static let service = "DD750B9A-9568-4FC3-B357-B7515AE46DED"
    /// Read: `{"name": ..., "psm": ...}`, answered per request.
    static let info = "820A3154-F1B2-47C2-B8BE-D89E812464A2"
    /// Write and notify: every write comes back as a notification.
    static let echo = "8D5498FD-CFE2-4D2C-99E9-1FDBC2D83892"
    /// Bytes each way in the L2CAP throughput test.
    static let l2capBytes = 1 << 20
    static let appGroup = "group.so.pen.herdr-spikes"
    /// An App Group works as a keychain access group, so the Notification
    /// Service Extension reads the key the app made without a team prefix.
    static let keychainGroup = appGroup
    static let notifyKeyAccount = "notify-key"
}
