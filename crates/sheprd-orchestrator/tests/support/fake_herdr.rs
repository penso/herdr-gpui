//! A fake `herdr` CLI for the orchestrator's dispatch tests, compiled with
//! `rustc` by the test that uses it so it runs on every client platform. It
//! logs each call's arguments to `$HOME/calls` and answers as Herdr would; its
//! state is files in `$HOME`:
//! - `fail`: a call whose arguments contain this text exits 1;
//! - `snapshot-default`, `snapshot`: `api snapshot` prints the second when it
//!   exists, else the first;
//! - `pane`: what `pane get` prints instead of pane `p9` on `term9`;
//! - `busy`: how many `agent start` calls are refused as a busy shell.

use std::{env, fs, io::Write, path::PathBuf, process::exit};

fn main() {
    let home = PathBuf::from(env::var_os("HOME").expect("HOME"));
    let args: Vec<String> = env::args().skip(1).collect();
    let line = args.join(" ");
    let mut calls = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(home.join("calls"))
        .expect("calls");
    writeln!(calls, "{line}").expect("calls");
    let fail = fs::read_to_string(home.join("fail")).unwrap_or_default();
    if !fail.is_empty() && line.contains(&fail) {
        exit(1);
    }
    let read = |name: &str| fs::read_to_string(home.join(name)).ok();
    let command = args.iter().take(2).map(String::as_str).collect::<Vec<_>>();
    let out = match command.as_slice() {
        ["api", "snapshot"] => read("snapshot")
            .or_else(|| read("snapshot-default"))
            .unwrap_or_default(),
        ["workspace", "create"] => r#"{"result":{"workspace":{"workspace_id":"w0"}}}"#.into(),
        ["worktree", "create"] => concat!(
            r#"{"result":{"workspace":{"workspace_id":"w9"},"tab":{"tab_id":"t9"},"#,
            r#""root_pane":{"pane_id":"p9","terminal_id":"term9"},"#,
            r#""worktree":{"path":"/tmp/wt/377-icons"}}}"#
        )
        .into(),
        ["pane", "get"] => read("pane").unwrap_or_else(|| {
            r#"{"result":{"pane":{"pane_id":"p9","terminal_id":"term9"}}}"#.into()
        }),
        ["agent", "start"] => {
            let busy: u32 = read("busy")
                .and_then(|busy| busy.trim().parse().ok())
                .unwrap_or(0);
            if busy > 0 {
                fs::write(home.join("busy"), format!("{}\n", busy - 1)).expect("busy");
                eprint!(
                    r#"{{"id":"cli:agent:start","error":{{"code":"agent_pane_busy","message":"not an available shell"}}}}"#
                );
                exit(1);
            }
            String::new()
        }
        _ => String::new(),
    };
    print!("{out}");
}
