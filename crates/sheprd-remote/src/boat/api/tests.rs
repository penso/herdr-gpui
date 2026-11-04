#![allow(clippy::unwrap_used)]
use super::*;

mod cancellation;
mod counts;
mod creation_recovery;
mod diagnostics;
mod identity;
mod lifecycle;

fn idempotency(request: &crate::cloud::tests::server::Request) -> &str {
    request
        .headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("idempotency-key")
                .then(|| value.trim())
        })
        .unwrap()
}

fn sandbox(state: &str, reason: Option<&str>) -> Sandbox {
    Sandbox {
        id: "bx_23456789".into(),
        name: "herdr-box".into(),
        state: state.into(),
        error: reason.map(str::to_owned),
    }
}

#[test]
fn states_map_to_what_connecting_needs() {
    for state in ["ready", "idle", "running"] {
        assert_eq!(sandbox(state, None).readiness(), Readiness::Ready);
    }
    assert_eq!(sandbox("archived", None).readiness(), Readiness::Stopped);
    assert_eq!(
        sandbox("provisioning", None).readiness(),
        Readiness::Pending("provisioning".into())
    );
    assert_eq!(
        sandbox("error", Some("out of quota")).readiness(),
        Readiness::Failed("out of quota".into())
    );
    assert_eq!(sandbox("cancelled", None).readiness(), Readiness::Deleted);
}

#[test]
fn ids_that_could_change_the_path_are_refused() {
    assert!(segment("bx_23456789").is_ok());
    for id in ["", "../x", "bx_a/b", "wrong", "bx_00000000", "bx_2345678"] {
        assert!(segment(id).is_err(), "{id}");
    }
}
