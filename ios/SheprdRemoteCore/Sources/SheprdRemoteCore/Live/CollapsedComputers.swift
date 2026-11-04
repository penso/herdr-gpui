/// The computers the Workspaces tab shows as their header alone. A computer
/// is open until the person collapses it, so a newly paired one shows.
public struct CollapsedComputers: Hashable, Sendable {
    public private(set) var devices: Set<String>

    public init(_ devices: some Sequence<String> = []) {
        self.devices = Set(devices)
    }

    public func contains(_ device: String) -> Bool {
        devices.contains(device)
    }

    public mutating func toggle(_ device: String) {
        if devices.remove(device) == nil { devices.insert(device) }
    }

    /// Whether every one of `shown` is collapsed; none shown never is.
    public func allCollapsed(_ shown: some Collection<String>) -> Bool {
        !shown.isEmpty && shown.allSatisfy(devices.contains)
    }

    /// Opens every computer when all of `shown` are collapsed, else
    /// collapses them all, so one still open never takes two taps to fold.
    public mutating func toggleAll(_ shown: some Collection<String>) {
        devices = allCollapsed(shown) ? [] : Set(shown)
    }

    /// A forgotten computer leaves nothing behind.
    public mutating func forget(_ device: String) {
        devices.remove(device)
    }
}
