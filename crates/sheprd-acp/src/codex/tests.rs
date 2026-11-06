#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

#[test]
fn codex_runs_its_app_server() {
    assert_eq!(args(), [OsString::from("app-server")]);
}
