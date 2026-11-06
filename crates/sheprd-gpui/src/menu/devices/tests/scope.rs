use crate::menu::DeviceScope;

const ENABLED: [&str; 3] = ["local", "ssh:a", "ssh:b"];

fn only<const N: usize>(ids: [&str; N]) -> DeviceScope {
    DeviceScope::Only(ids.into_iter().map(str::to_owned).collect())
}

#[test]
fn picking_from_all_devices_keeps_the_selected_device_listed() {
    let scope = DeviceScope::All.toggled("ssh:a", "local", ENABLED);
    assert_eq!(scope, Some(only(["local", "ssh:a"])));
    // Picking the device on screen from All Devices lists it alone.
    let scope = DeviceScope::All.toggled("local", "local", ENABLED);
    assert_eq!(scope, Some(DeviceScope::only("local")));
}

#[test]
fn toggling_adds_and_takes_away_devices_but_never_lists_nothing() {
    let picked = only(["local", "ssh:a"]);
    assert_eq!(
        picked.toggled("ssh:a", "local", ENABLED),
        Some(DeviceScope::only("local"))
    );
    assert_eq!(
        picked.toggled("local", "local", ENABLED),
        Some(DeviceScope::only("ssh:a")),
        "the caller selects a listed device"
    );
    assert_eq!(
        DeviceScope::only("local").toggled("local", "local", ENABLED),
        None
    );
}

#[test]
fn naming_every_enabled_device_is_all_devices() {
    let picked = only(["local", "ssh:a"]);
    assert_eq!(
        picked.toggled("ssh:b", "local", ENABLED),
        Some(DeviceScope::All)
    );
    // A disabled device is not needed for the scope to be everything.
    assert_eq!(
        picked.toggled("ssh:b", "local", ["local", "ssh:b"]),
        Some(DeviceScope::All)
    );
}

#[test]
fn reconcile_drops_devices_that_left_without_widening_the_scope() {
    let mut scope = only(["local", "ssh:a", "ssh:gone"]);
    scope.reconcile(ENABLED);
    assert_eq!(scope, only(["local", "ssh:a"]));
    assert!(!scope.shows("ssh:gone"));
    // A device that comes back later stays hidden until it is picked.
    scope.reconcile(["local", "ssh:a"]);
    assert_eq!(scope, only(["local", "ssh:a"]));
    let mut scope = DeviceScope::only("ssh:gone");
    scope.reconcile(ENABLED);
    assert_eq!(scope, DeviceScope::All, "an empty scope lists everything");
}

#[test]
fn navigation_keeps_a_scope_that_lists_the_device() {
    let mut scope = only(["local", "ssh:a"]);
    scope.follow("ssh:a");
    assert_eq!(scope, only(["local", "ssh:a"]));
    scope.follow("ssh:b");
    assert_eq!(scope, DeviceScope::only("ssh:b"));
    let mut scope = DeviceScope::All;
    scope.follow("ssh:b");
    assert_eq!(scope, DeviceScope::All);
}

#[test]
fn only_named_devices_are_picked() {
    assert!(!DeviceScope::All.picks("local"));
    assert_eq!(DeviceScope::All.picked().count(), 0);
    let scope = only(["local", "ssh:a"]);
    assert!(scope.picks("ssh:a"));
    assert!(!scope.picks("ssh:b"));
    assert_eq!(scope.picked().collect::<Vec<_>>(), ["local", "ssh:a"]);
}
