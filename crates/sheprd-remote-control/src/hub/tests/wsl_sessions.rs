//! Sessions inside WSL distributions: which are followed, how their ids read,
//! and what a phone cannot do there.

use super::*;
use crate::hub::{
    layout::{Chats, project_with},
    sessions::{MAX_WSL_SESSIONS, Sessions, in_wsl, owner, wsl_wanted},
};
use sheprd_client::{ConnectTarget, RemoteSession, WslHost};

const ENDPOINT: &str = "0123456789abcdef0123456789abcdef";

fn host(distro: &str) -> WslHost {
    WslHost {
        distro: distro.to_owned(),
        session: "default".to_owned(),
    }
}

fn session(name: &str, running: bool) -> RemoteSession {
    RemoteSession {
        name: name.to_owned(),
        running,
    }
}

#[test]
fn only_running_sessions_of_saved_running_distributions_are_followed() {
    let saved = [host("Ubuntu"), host("Debian")];
    let running = ["Ubuntu".to_owned(), "Fedora".to_owned()];
    let mut asked = Vec::new();
    let wanted = wsl_wanted(&saved, &running, |distro| {
        asked.push(distro.to_owned());
        vec![session("default", true), session("old", false)]
    });
    // A stopped distribution is never asked: asking would boot it.
    assert_eq!(asked, ["Ubuntu"]);
    assert_eq!(
        wanted,
        [(
            "default@Ubuntu".to_owned(),
            ConnectTarget::Wsl {
                distro: "Ubuntu".to_owned(),
                session: "default".to_owned(),
            }
        )]
    );
}

#[test]
fn the_sessions_followed_are_bounded() {
    let saved: Vec<_> = (0..64).map(|index| host(&format!("d{index}"))).collect();
    let running: Vec<_> = saved.iter().map(|host| host.distro.clone()).collect();
    let wanted = wsl_wanted(&saved, &running, |_| {
        (0..64)
            .map(|index| session(&format!("s{index}"), true))
            .collect()
    });
    assert!(wanted.len() < 64);
}

/// A session sorting first pushes one out of the scan, which stays followed
/// because it still answers, so the scan's own bound does not hold here.
#[tokio::test]
async fn a_session_listed_later_waits_while_the_followed_ones_fill_the_limit() {
    let followed: Vec<_> = (0..MAX_WSL_SESSIONS)
        .map(|index| format!("s{index}@Ubuntu"))
        .collect();
    let sessions = Sessions::offline_named(followed.clone());
    let newcomer = "a@Ubuntu";
    let target = |session: &str| ConnectTarget::Wsl {
        distro: "Ubuntu".to_owned(),
        session: session.to_owned(),
    };
    let wanted: Vec<_> = std::iter::once(newcomer)
        .chain(followed.iter().map(String::as_str))
        .take(MAX_WSL_SESSIONS)
        .map(|name| (name.to_owned(), target(name)))
        .collect();

    sessions.update(&wanted);

    assert!(sessions.get(newcomer).is_none());
    assert!(followed.iter().all(|name| sessions.get(name).is_some()));
}

#[test]
fn a_wsl_session_routes_like_a_named_one_and_is_told_apart() {
    let (name, params) = owner(&json!({"pane_id": "herdr:default@Ubuntu:p1"}))
        .unwrap()
        .unwrap();
    assert_eq!(name, "default@Ubuntu");
    assert_eq!(params, json!({"pane_id": "p1"}));
    assert!(in_wsl("herdr:default@Ubuntu:w1"));
    assert!(!in_wsl("herdr:work:w1"));
    assert!(!in_wsl("w1"));
}

#[test]
fn a_chat_in_a_distribution_is_listed_as_the_terminal_it_also_is() {
    let snapshot = snapshot(json!([agent("p1", "t1", json!([["acp", ENDPOINT]]))]));
    let layout = project_with(&snapshot, Chats::Terminals);
    let workspace = &layout.workspaces[0];
    assert!(workspace.chats.is_empty());
    assert_eq!(workspace.terminals[0].tab, "t1");
    assert_eq!(workspace.terminals[0].agent.as_ref().unwrap().pane, "p1");
    assert_eq!(
        project_with(&snapshot, Chats::Linked).workspaces[0]
            .chats
            .len(),
        1
    );
}

#[tokio::test]
async fn a_chat_is_refused_in_a_distribution_whatever_else_could_run_it() {
    let hub = Hub::offline(Layout::default());
    let refused = hub
        .start_chat("herdr:default@Ubuntu:w1", Agent::Codex, None)
        .await;
    assert!(matches!(refused, Err(Error::WslChat)));
}

#[tokio::test]
async fn a_new_workspace_is_refused_in_a_distribution() {
    let hub = Hub::offline(Layout::default());
    let refused = hub
        .create_workspace("/home/me/repo", None, Agent::Codex, Some("default@Ubuntu"))
        .await;
    assert!(matches!(refused, Err(Error::WslChat)));
}

#[tokio::test]
async fn only_a_pane_in_a_distribution_has_a_path_mapped() {
    let hub = Hub::offline(Layout::default());
    let path = Path::new("picture.png");
    assert!(matches!(hub.wsl_path("p1", path).await, Ok(None)));
    assert!(matches!(
        hub.wsl_path("herdr:work:p1", path).await,
        Ok(None)
    ));
    // Never the path unchanged: a distribution could not open it.
    assert!(matches!(
        hub.wsl_path("herdr:default@Ubuntu:p1", path).await,
        Err(Error::NotConnected)
    ));
}

#[tokio::test]
async fn a_new_tab_in_a_distribution_is_never_the_cores() {
    let hub = Hub::offline(Layout::default());
    assert!(matches!(
        hub.new_core_tab("herdr:default@Ubuntu:w1").await,
        Ok(None)
    ));
}

/// No `wsl.exe` here, so the bridge fails as one to a stopped session does.
#[cfg(not(windows))]
#[tokio::test]
async fn a_session_that_went_away_is_not_retried_until_it_is_listed_again() {
    let name = "default@Ubuntu";
    let wanted = [(
        name.to_owned(),
        ConnectTarget::Wsl {
            distro: "Ubuntu".to_owned(),
            session: "default".to_owned(),
        },
    )];
    let sessions = Sessions::offline();
    sessions.update(&wanted);
    let first = sessions.get(name).unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while !first.ended() {
        assert!(std::time::Instant::now() < deadline, "the worker retried");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }

    // Still listed: followed by a new worker, not left holding the dead one.
    sessions.update(&wanted);
    assert!(!Arc::ptr_eq(&first, &sessions.get(name).unwrap()));

    // No longer listed: let go, whatever its worker is doing.
    sessions.update(&[]);
    assert!(sessions.get(name).is_none());
}
