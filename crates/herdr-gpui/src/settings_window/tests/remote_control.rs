//! Settings > Remote control: the switch and settings save `[remote]`
//! edits, the page shows what the daemon reports, and Revoke asks before it
//! sends anything.
use super::*;
use crate::{
    config::{
        RemoteConfig,
        preferences::Preference,
        remote::{Bind, Edit},
    },
    remote::{Health, Supervisor},
    settings_window::{
        controls::preferences::PreferenceIo,
        inputs::NumberField,
        pages::remote_control::{bind_choices, device_line, health_line},
        ui::NumberChange,
    },
};
use herdr_remote_control::control::{self, Device, DeviceId, Request, Response, Status};
use std::path::Path;

fn status() -> Status {
    Status {
        version: "dev".into(),
        port: 7443,
        addresses: vec![
            "https://192.168.1.20:7443".into(),
            "https://[fd7a:115c::3]:7443".into(),
        ],
        fingerprint: "5F:2A:C1:9E".into(),
        herdr: true,
        chats: 2,
        devices: 2,
        connected: 1,
    }
}

fn devices() -> Vec<Device> {
    vec![
        Device {
            id: DeviceId("d1".into()),
            name: "Pixel".into(),
            created_ms: 0,
            last_seen_ms: Some(0),
            push: true,
            connected: true,
        },
        Device {
            id: DeviceId("d2".into()),
            name: "iPad".into(),
            created_ms: 0,
            last_seen_ms: None,
            push: false,
            connected: false,
        },
    ]
}

static REVOKED: Mutex<Vec<Request>> = Mutex::new(Vec::new());

fn daemon(_: &Path, request: &Request) -> Result<Response, control::Error> {
    if matches!(request, Request::Revoke { .. }) {
        REVOKED.lock().unwrap().push(request.clone());
    }
    Ok(Response::Done)
}

type Edits = Arc<Mutex<Vec<Preference>>>;

/// The page with remote control on, the daemon running with two devices,
/// and every save recorded.
fn page(cx: &mut TestAppContext) -> (Entity<SettingsWindow>, &mut VisualTestContext, Edits) {
    let (view, cx) = cx.add_window_view(skill_fixture);
    let config = RemoteConfig {
        enabled: true,
        ..RemoteConfig::default()
    };
    cx.update(|_, cx| {
        Supervisor::fixture(cx, config.clone(), daemon);
        Supervisor::set_health(cx, Health::Running(status()), Some(devices()));
    });
    let edits = Edits::default();
    let captured = edits.clone();
    view.update_in(cx, |view, window, cx| {
        view.controls.preference_io = Some(PreferenceIo::recording(move |edit| {
            captured.lock().unwrap().push(edit)
        }));
        view.config.remote = config;
        view.select_section(Section::RemoteControl, window, cx);
    });
    cx.simulate_resize(size(px(960.), px(2400.)));
    draw(cx);
    (view, cx, edits)
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let point = cx.debug_bounds(selector).unwrap().center();
    cx.simulate_click(point, Default::default());
    cx.run_until_parked();
    draw(cx);
}

#[gpui::test]
fn the_page_shows_what_the_daemon_reports(cx: &mut TestAppContext) {
    let (view, cx, _) = page(cx);
    for selector in [
        "remote-enabled-toggle",
        "remote-keep-running-toggle",
        "remote-port-control",
        "remote-bind-control",
        "remote-addresses",
        "remote-certificate-control-0",
        "remote-notifications-control-0",
        "remote-idle-control-0",
        "remote-idle-control-3",
        "remote-pair-button",
        "remote-device-0",
        "remote-device-1",
        "remote-device-1-revoke",
    ] {
        assert!(cx.debug_bounds(selector).is_some(), "{selector}");
    }
    assert!(cx.debug_bounds("remote-device-2").is_none());
    // A question left open does not survive leaving the page.
    click(cx, "remote-device-0-revoke");
    view.update_in(cx, |view, window, cx| {
        assert!(view.remote.revoking.is_some());
        view.select_section(Section::General, window, cx);
    });
    view.read_with(cx, |view, _| assert!(view.remote.revoking.is_none()));
    assert!(
        REVOKED
            .lock()
            .unwrap()
            .iter()
            .all(|request| { !matches!(request, Request::Revoke { device } if device.0 == "d1") })
    );
}

#[gpui::test]
fn switches_and_choices_save_remote_edits(cx: &mut TestAppContext) {
    let (view, cx, edits) = page(cx);
    click(cx, "remote-enabled-toggle");
    click(cx, "remote-keep-running-toggle");
    click(cx, "remote-notifications-control-1");
    assert_eq!(
        *edits.lock().unwrap(),
        [
            Preference::Remote(Edit::Enabled(false)),
            Preference::Remote(Edit::KeepRunning(false)),
            Preference::Remote(Edit::Notifications(crate::config::remote::Privacy::Excerpt)),
        ]
    );
    edits.lock().unwrap().clear();
    view.update(cx, |view, cx| {
        view.commit_number(NumberField::RemotePort, NumberChange::Commit(8443.), cx);
    });
    cx.run_until_parked();
    click(cx, "remote-idle-control-3");
    view.update(cx, |view, cx| view.pick_remote_bind(1, cx));
    cx.run_until_parked();
    assert_eq!(
        *edits.lock().unwrap(),
        [
            Preference::Remote(Edit::Port(8443)),
            Preference::Remote(Edit::DeviceIdleDays(0)),
            Preference::Remote(Edit::Bind(Bind::Address("192.168.1.20".parse().unwrap()))),
        ]
    );
}

#[gpui::test]
fn revoke_asks_on_the_page_before_it_sends(cx: &mut TestAppContext) {
    let (view, cx, _) = page(cx);
    click(cx, "remote-device-1-revoke");
    view.read_with(cx, |view, _| {
        assert_eq!(view.remote.revoking, Some(DeviceId("d2".into())));
    });
    assert!(
        REVOKED.lock().unwrap().is_empty(),
        "the first click only asks"
    );
    click(cx, "remote-device-1-cancel");
    view.read_with(cx, |view, _| assert!(view.remote.revoking.is_none()));
    click(cx, "remote-device-1-revoke");
    click(cx, "remote-device-1-revoke");
    assert_eq!(
        *REVOKED.lock().unwrap(),
        [Request::Revoke {
            device: DeviceId("d2".into())
        }]
    );
    view.read_with(cx, |view, _| assert!(view.remote.revoking.is_none()));
}

#[gpui::test]
fn pairing_needs_remote_control_on(cx: &mut TestAppContext) {
    let (view, cx, _) = page(cx);
    view.update(cx, |view, cx| {
        view.config.remote.enabled = false;
        cx.notify();
    });
    draw(cx);
    let before = view.read_with(cx, |view, _| view.error.clone());
    click(cx, "remote-pair-button");
    view.read_with(cx, |view, _| assert_eq!(view.error, before));
}

#[test]
fn bind_choices_come_from_the_addresses_served() {
    let running = Health::Running(status());
    assert_eq!(
        bind_choices(Bind::All, &running),
        [
            Bind::All,
            Bind::Address("192.168.1.20".parse().unwrap()),
            Bind::Address("fd7a:115c::3".parse().unwrap()),
        ]
    );
    let other = Bind::Address("10.0.0.9".parse().unwrap());
    assert_eq!(bind_choices(other, &Health::Off), [Bind::All, other]);
}

#[test]
fn the_switch_row_says_how_the_daemon_stands() {
    assert!(health_line(&Health::Off, false).starts_with("Off."));
    assert_eq!(
        health_line(&Health::Off, true),
        "Updating the service\u{2026}"
    );
    assert_eq!(
        health_line(&Health::Running(status()), false),
        "Running on port 7443 \u{b7} 2 devices, 1 connected"
    );
    let mut alone = status();
    alone.herdr = false;
    alone.devices = 1;
    assert_eq!(
        health_line(&Health::Running(alone), false),
        "Running on port 7443 \u{b7} 1 device, 1 connected \u{b7} Herdr is not running"
    );
    assert_eq!(
        health_line(&Health::NotRunning("refused".into()), false),
        "Not running: refused"
    );
}

#[test]
fn a_device_line_says_when_it_was_seen() {
    let [pixel, ipad] = <[Device; 2]>::try_from(devices()).unwrap();
    assert_eq!(
        device_line(&pixel, 5_000),
        "Connected now \u{b7} notifications on"
    );
    assert_eq!(
        device_line(&ipad, 5_000),
        "Never connected \u{b7} notifications off"
    );
    let away = Device {
        connected: false,
        last_seen_ms: Some(0),
        ..pixel
    };
    assert_eq!(
        device_line(&away, 2 * 86_400_000),
        "Last seen 2 days ago \u{b7} notifications on"
    );
}

static RESTARTED: AtomicUsize = AtomicUsize::new(0);

#[gpui::test]
fn restart_reloads_the_daemon_only_while_on(cx: &mut TestAppContext) {
    let (view, cx, _) = page(cx);
    cx.update(|_, cx| {
        cx.global_mut::<Supervisor>().set_restart_for_tests(|_| {
            RESTARTED.fetch_add(1, Ordering::SeqCst);
            Ok(crate::remote::service::Outcome::Started)
        });
    });
    click(cx, "remote-restart-button");
    assert_eq!(RESTARTED.load(Ordering::SeqCst), 1);
    view.update(cx, |view, cx| {
        view.config.remote.enabled = false;
        cx.notify();
    });
    draw(cx);
    click(cx, "remote-restart-button");
    assert_eq!(RESTARTED.load(Ordering::SeqCst), 1);
}
