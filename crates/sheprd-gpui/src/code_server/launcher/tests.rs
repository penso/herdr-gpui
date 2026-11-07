#![allow(clippy::unwrap_used)]
use super::*;
use crate::code_server::supervisor::Address;
use std::num::NonZeroU16;

fn found() -> Cli {
    Cli::Found(PathBuf::from("/Applications/VS Code/bin/code-tunnel"))
}

fn url(text: &str) -> WebUrl {
    WebUrl::try_from(text).unwrap()
}

fn accepted() -> CodeConfig {
    CodeConfig {
        license_accepted: true,
        ..CodeConfig::default()
    }
}

fn report(status: Status, port: Option<u16>) -> Report {
    Report {
        revision: 1,
        status,
        address: port.map(|port| Address {
            port: NonZeroU16::new(port).unwrap(),
            url: url(&format!("http://127.0.0.1:{port}/?tkn=secret")),
        }),
    }
}

#[test]
fn starting_is_the_default_only_once_vs_code_is_found() {
    let blank = CodeConfig::default();
    assert_eq!(mode(&blank, &found()), Some(CodeMode::Start));
    assert_eq!(mode(&blank, &Cli::Missing), Some(CodeMode::Address));
    assert_eq!(mode(&blank, &Cli::Finding), None);
    assert_eq!(mode(&blank, &Cli::Unknown), None);
    // A config from before the choice keeps its address.
    let older = CodeConfig {
        url: Some(url("http://127.0.0.1:8000/?tkn=x")),
        ..CodeConfig::default()
    };
    for cli in [found(), Cli::Missing, Cli::Finding] {
        assert_eq!(mode(&older, &cli), Some(CodeMode::Address));
    }
    // A saved choice wins either way.
    let start = CodeConfig {
        mode: Some(CodeMode::Start),
        ..older.clone()
    };
    assert_eq!(mode(&start, &Cli::Missing), Some(CodeMode::Start));
    let address = CodeConfig {
        mode: Some(CodeMode::Address),
        ..CodeConfig::default()
    };
    assert_eq!(mode(&address, &found()), Some(CodeMode::Address));
}

#[test]
fn nothing_starts_until_the_license_is_accepted() {
    let token = PathBuf::from("/state/vscode-token");
    let blank = CodeConfig::default();
    assert!(plan(true, &blank, &found(), token.clone()).is_none());
    assert!(matches!(
        startup(true, &blank, &found(), false, None),
        Startup::Consent
    ));
    let plan = plan(true, &accepted(), &found(), token.clone()).unwrap();
    assert_eq!(
        plan.program,
        PathBuf::from("/Applications/VS Code/bin/code-tunnel")
    );
    assert_eq!(plan.token_file, token);
    assert_eq!(plan.port, None);
    assert!(matches!(
        startup(true, &accepted(), &found(), false, None),
        Startup::Idle
    ));
}

#[test]
fn builds_without_pages_find_and_start_nothing() {
    let start = CodeConfig {
        mode: Some(CodeMode::Start),
        ..accepted()
    };
    let token = PathBuf::from("/state/vscode-token");
    assert!(plan(false, &start, &found(), token).is_none());
    let ready = report(Status::Ready, Some(51234));
    assert!(matches!(
        startup(false, &start, &found(), true, Some(&ready)),
        Startup::Address
    ));
}

#[test]
fn an_address_or_a_missing_vs_code_starts_nothing() {
    let token = PathBuf::from("/state/vscode-token");
    let address = CodeConfig {
        mode: Some(CodeMode::Address),
        ..accepted()
    };
    assert!(plan(true, &address, &found(), token.clone()).is_none());
    let start = CodeConfig {
        mode: Some(CodeMode::Start),
        ..accepted()
    };
    assert!(plan(true, &start, &Cli::Missing, token).is_none());
    assert!(matches!(
        startup(true, &start, &Cli::Missing, false, None),
        Startup::Missing
    ));
    assert!(matches!(
        startup(true, &start, &Cli::Finding, false, None),
        Startup::Finding
    ));
}

#[test]
fn the_worker_report_says_where_the_server_is() {
    let code = accepted();
    let cli = found();
    let starting = report(Status::Starting, None);
    assert!(matches!(
        startup(true, &code, &cli, true, None),
        Startup::Starting { url: None }
    ));
    assert!(matches!(
        startup(true, &code, &cli, true, Some(&starting)),
        Startup::Starting { url: None }
    ));
    let ready = report(Status::Ready, Some(51234));
    let Startup::Ready { url: ready_url } = startup(true, &code, &cli, true, Some(&ready)) else {
        panic!("not ready");
    };
    assert_eq!(ready_url.address(), "127.0.0.1:51234");
    // A server that stopped keeps its address, so its pages wait for it.
    let failed = report(
        Status::Failed {
            error: Arc::new(crate::code_server::Error::PortTaken { port: 51234 }.into()),
            retry: Instant::now(),
        },
        Some(51234),
    );
    let stopped = startup(true, &code, &cli, true, Some(&failed));
    assert!(matches!(stopped, Startup::Failed { .. }));
    assert_eq!(
        stopped.url(&code).map(WebUrl::address).as_deref(),
        Some("127.0.0.1:51234")
    );
    assert!(!stopped.serving());
    assert!(stopped.offered(&code));
}

#[test]
fn the_title_bar_offers_the_panel_once_there_is_a_server() {
    let blank = CodeConfig::default();
    assert!(!Startup::Address.offered(&blank));
    assert!(!Startup::Finding.offered(&blank));
    let address = CodeConfig {
        url: Some(url("http://127.0.0.1:8000/?tkn=x")),
        ..CodeConfig::default()
    };
    assert!(Startup::Address.offered(&address));
    assert_eq!(
        Startup::Address.url(&address).map(WebUrl::as_str),
        Some("http://127.0.0.1:8000/?tkn=x")
    );
    for startup in [Startup::Missing, Startup::Consent, Startup::Idle] {
        assert!(startup.offered(&blank));
        assert!(startup.url(&blank).is_none());
        assert!(!startup.serving());
    }
}

#[gpui::test]
fn without_a_launcher_vs_code_counts_as_not_installed(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        let start = CodeConfig {
            mode: Some(CodeMode::Start),
            ..accepted()
        };
        Launcher::want(cx, &start);
        assert!(!cx.has_global::<Launcher>());
        let expected = if EMBEDDED { "Missing" } else { "Address" };
        assert_eq!(format!("{:?}", Launcher::startup(cx, &start)), expected);
    });
}

/// Linux shows no pages, so even a found VS Code and an accepted license
/// start nothing there.
#[cfg(not(any(target_os = "macos", windows)))]
#[gpui::test]
fn linux_starts_nothing(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        Launcher::fixture(cx, found());
        Launcher::want(cx, &accepted());
        assert!(!Launcher::running(cx));
        assert!(cx.global::<Launcher>().report.is_none());
    });
}

// Only builds that show pages start anything.
#[cfg(any(target_os = "macos", windows))]
#[gpui::test]
fn a_running_server_is_kept_unless_its_program_or_port_changes(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        Launcher::fixture(cx, found());
        let code = accepted();
        Launcher::want(cx, &code);
        assert!(Launcher::running(cx));
        // The port it picked and saved comes back with the config, which
        // may be before the poll took the worker's report.
        let picked = report(Status::Ready, Some(51234)).address.unwrap();
        let launcher = cx.global::<Launcher>();
        launcher.server.as_ref().unwrap().report_address(picked);
        assert!(Launcher::report(cx).is_none(), "not polled yet");
        let saved = CodeConfig {
            port: NonZeroU16::new(51234),
            ..code.clone()
        };
        Launcher::want(cx, &saved);
        let kept = cx.global::<Launcher>().server.as_ref().unwrap().report();
        assert_eq!(kept.address.unwrap().port.get(), 51234, "kept");
        // Another port starts it anew.
        let moved = CodeConfig {
            port: NonZeroU16::new(51235),
            ..code
        };
        Launcher::want(cx, &moved);
        assert!(Launcher::running(cx));
        let fresh = cx.global::<Launcher>().server.as_ref().unwrap().report();
        assert!(fresh.address.is_none(), "started anew");
        let launcher = cx.global::<Launcher>();
        assert_eq!(
            launcher.server.as_ref().unwrap().plan().port,
            NonZeroU16::new(51235)
        );
        // Choosing an address stops it.
        Launcher::stop(cx);
        assert!(!Launcher::running(cx));
    });
}

// Only builds that show pages start anything.
#[cfg(any(target_os = "macos", windows))]
#[gpui::test]
fn a_start_that_fails_is_tried_again_after_a_while(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        Launcher::fixture(cx, found());
        cx.global_mut::<Launcher>().start =
            |_| Err(crate::code_server::Error::Worker(std::io::ErrorKind::Other.into()).into());
        Launcher::want(cx, &accepted());
        assert!(!Launcher::running(cx));
        let Some(Report {
            status: Status::Failed { error, retry },
            ..
        }) = Launcher::report(cx)
        else {
            panic!("not failed");
        };
        assert_eq!(error.to_string(), "Could not start the VS Code worker.");
        assert!(retry > Instant::now());
        // Not before then.
        cx.global_mut::<Launcher>().start = Supervisor::idle;
        Launcher::want(cx, &accepted());
        assert!(!Launcher::running(cx));
        if let Some(Report {
            status: Status::Failed { retry, .. },
            ..
        }) = &mut cx.global_mut::<Launcher>().report
        {
            *retry = Instant::now();
        }
        Launcher::want(cx, &accepted());
        assert!(Launcher::running(cx));
    });
}

/// A config that no longer starts VS Code stops it, however it changed.
#[cfg(any(target_os = "macos", windows))]
#[gpui::test]
fn a_config_that_no_longer_starts_vs_code_stops_it(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        Launcher::fixture(cx, found());
        let code = accepted();
        Launcher::want(cx, &code);
        Launcher::sync(cx, &code);
        assert!(Launcher::running(cx), "still asked for");
        let address = CodeConfig {
            mode: Some(CodeMode::Address),
            ..code.clone()
        };
        Launcher::sync(cx, &address);
        assert!(!Launcher::running(cx));

        Launcher::want(cx, &code);
        let declined = CodeConfig {
            license_accepted: false,
            ..code
        };
        Launcher::sync(cx, &declined);
        assert!(!Launcher::running(cx));
    });
}
