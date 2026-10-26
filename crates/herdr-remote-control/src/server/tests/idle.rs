//! The expiry sweep: Never keeps devices while old pictures still go.

use super::*;

const DAY_MS: u64 = 24 * 60 * 60 * 1000;
const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0];

fn paired(dir: &std::path::Path) -> (Arc<State>, String, u64) {
    let state = web::tests::state(dir);
    web::tests::signed_in(&state, 7);
    let now = web::now_ms();
    let upload = state.uploads.store("old".into(), PNG, now).unwrap();
    (state, upload, now)
}

fn devices(state: &State) -> usize {
    state.auth().devices().list(|_| false).len()
}

fn upload_exists(state: &State, id: &str, now_ms: u64) -> bool {
    let attachment = herdr_chat_protocol::Attachment {
        path: id.to_owned(),
    };
    state.uploads.resolve(&[attachment], now_ms).is_ok()
}

#[test]
fn never_keeps_an_idle_device_and_still_prunes_old_pictures() {
    let dir = tempfile::tempdir().unwrap();
    let (state, upload, now) = paired(dir.path());
    let later = now + 400 * DAY_MS;
    sweep(&state, later, None);
    assert_eq!(devices(&state), 1);
    // Back-dated so only a prune, not the lifetime check, can hide it.
    assert!(!upload_exists(&state, &upload, now));
    assert!(
        !dir.path()
            .join("uploads")
            .join(format!("{upload}.png"))
            .exists()
    );
}

#[test]
fn a_limit_forgets_a_device_away_longer() {
    let dir = tempfile::tempdir().unwrap();
    let (state, _, now) = paired(dir.path());
    sweep(&state, now + DAY_MS / 2, Some(DAY_MS));
    assert_eq!(devices(&state), 1);
    sweep(&state, now + 2 * DAY_MS, Some(DAY_MS));
    assert_eq!(devices(&state), 0);
}
