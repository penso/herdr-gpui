#![allow(clippy::unwrap_used)]
use super::*;
use crate::browser::WebUrl;

mod origin;
mod script;

fn accepted() -> CodeConfig {
    CodeConfig {
        license_accepted: true,
        ..CodeConfig::default()
    }
}

#[test]
fn each_host_keeps_its_own_port_and_token() {
    let (one, two) = (port_of("dev@build"), port_of("dev@other"));
    assert!(PORTS.contains(&one.get()) && PORTS.contains(&two.get()));
    assert_eq!(one, port_of("dev@build"), "the same each launch");
    assert_ne!(one, two);
    assert_ne!(fingerprint("dev@build"), fingerprint("dev@other"));
}

#[test]
fn socket_names_are_fresh_and_ones_ssh_takes() {
    let (one, two) = (socket_name().unwrap(), socket_name().unwrap());
    assert_ne!(one, two);
    assert!(one.starts_with("sheprd-vscode-"), "{one}");
    assert!(sheprd_client::serve_command("host", 20_000, &one, SCRIPT).is_ok());
}

#[test]
fn the_ssh_forwards_the_port_and_carries_no_token() {
    let plan = Plan {
        program: PathBuf::from("ssh"),
        port: None,
        token_file: PathBuf::from("vscode-token-1"),
        host: Some("dev@build".into()),
    };
    let ssh = command(&plan, NonZeroU16::new(24_567).unwrap()).unwrap();
    let args: Vec<_> = ssh
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    assert!(
        args.iter()
            .any(|arg| arg.starts_with("127.0.0.1:24567:/tmp/sheprd-vscode-")),
        "{args:?}"
    );
    assert!(args.contains(&"dev@build".to_owned()));
    assert!(!args.iter().any(|arg| arg.contains("tkn")), "{args:?}");
    // A plan without a host is refused rather than run anywhere.
    let local = Plan { host: None, ..plan };
    assert!(command(&local, NonZeroU16::MIN).is_err());
}

#[cfg(unix)]
#[test]
fn the_scripts_exit_codes_name_what_went_wrong() {
    use std::os::unix::process::ExitStatusExt;
    let named = |code: i32| match exited(ExitStatus::from_raw(code << 8)) {
        super::super::Error::Host(error) => Some(error),
        _ => None,
    };
    assert!(matches!(named(2), Some(Error::Setup)));
    assert!(matches!(named(3), Some(Error::Unsupported)));
    assert!(matches!(named(4), Some(Error::NoDownloader)));
    assert!(matches!(named(5), Some(Error::Download)));
    assert!(matches!(named(6), Some(Error::Unpack)));
    assert!(matches!(named(255), Some(Error::Ssh)));
    assert!(matches!(
        exited(ExitStatus::from_raw(1 << 8)),
        super::super::Error::Exited(_)
    ));
}

#[test]
fn hosts_get_servers_only_when_the_app_starts_vs_code() {
    let address = CodeConfig {
        url: Some(WebUrl::try_from("http://127.0.0.1:8000/?tkn=x").unwrap()),
        ..accepted()
    };
    assert!(!serves(&address), "an address serves every workspace");
    let start = CodeConfig {
        mode: Some(CodeMode::Start),
        ..address.clone()
    };
    assert_eq!(serves(&start), EMBEDDED);
    assert_eq!(serves(&accepted()), EMBEDDED);
    let chosen = CodeConfig {
        mode: Some(CodeMode::Address),
        ..accepted()
    };
    assert!(!serves(&chosen));
}

#[gpui::test]
fn without_the_app_starting_vs_code_a_host_uses_the_local_server(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        assert!(
            Hosts::startup(cx, &accepted(), "dev@build").is_none(),
            "not installed"
        );
        Hosts::fixture(cx);
        let address = CodeConfig {
            mode: Some(CodeMode::Address),
            ..accepted()
        };
        assert!(Hosts::startup(cx, &address, "dev@build").is_none());
        Hosts::want(cx, &address, "dev@build");
        assert!(Hosts::plan(cx, "dev@build").is_none());
    });
}

// Only builds that show pages start anything.
#[cfg(any(target_os = "macos", windows))]
#[gpui::test]
fn a_host_starts_once_its_license_is_accepted(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        Hosts::fixture(cx);
        let declined = CodeConfig::default();
        assert!(matches!(
            Hosts::startup(cx, &declined, "dev@build"),
            Some(Startup::Consent)
        ));
        Hosts::want(cx, &declined, "dev@build");
        assert!(Hosts::plan(cx, "dev@build").is_none());

        let code = accepted();
        assert!(matches!(
            Hosts::startup(cx, &code, "dev@build"),
            Some(Startup::Idle)
        ));
        Hosts::want(cx, &code, "dev@build");
        let plan = Hosts::plan(cx, "dev@build").unwrap();
        assert_eq!(plan.host.as_deref(), Some("dev@build"));
        assert_eq!(plan.port, None);
        assert!(matches!(
            Hosts::startup(cx, &code, "dev@build"),
            Some(Startup::Starting { url: None })
        ));
        // Another host has a server and a token of its own.
        Hosts::want(cx, &code, "dev@other");
        let other = Hosts::plan(cx, "dev@other").unwrap();
        assert_ne!(other.token_file, plan.token_file);
        assert!(matches!(
            Hosts::startup(cx, &code, "dev@nowhere"),
            Some(Startup::Idle)
        ));
        // A config that no longer starts VS Code stops them all.
        Hosts::sync(cx, &code);
        assert!(Hosts::plan(cx, "dev@build").is_some(), "still asked for");
        Hosts::sync(cx, &CodeConfig::default());
        assert!(Hosts::plan(cx, "dev@build").is_none());
        assert!(Hosts::plan(cx, "dev@other").is_none());
    });
}

#[cfg(any(target_os = "macos", windows))]
#[gpui::test]
fn a_host_that_failed_to_start_waits_before_trying_again(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        Hosts::fixture(cx);
        cx.global_mut::<Hosts>().start = |_| Err(super::super::Error::Host(Error::Ssh).into());
        let code = accepted();
        Hosts::want(cx, &code, "dev@build");
        let Some(Startup::Failed { error, .. }) = Hosts::startup(cx, &code, "dev@build") else {
            panic!("not failed");
        };
        assert!(matches!(
            &*error,
            crate::Error::Code(super::super::Error::Host(Error::Ssh))
        ));
        cx.global_mut::<Hosts>().start = Supervisor::idle;
        Hosts::want(cx, &code, "dev@build");
        assert!(Hosts::plan(cx, "dev@build").is_none(), "not before then");
        Hosts::set_report(cx, "dev@build", None);
        Hosts::want(cx, &code, "dev@build");
        assert!(Hosts::plan(cx, "dev@build").is_some());
    });
}
