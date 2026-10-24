//! The dialog's Arguments field: filled from the config or the last
//! dispatch, kept per agent, and split into the request's arguments.
use super::*;
use crate::orchestrator::{
    Activity, Item, ItemKey, Provider, PullRequest, Repository, SourceKey,
    repo::{Remote, RepoInfo},
};
use gpui::{Entity, VisualTestContext};

/// The `[orchestrator]` table.
const CONFIG: &str = r#"
[agents.opencode]
args = ["--standalone"]
review_args = ["--standalone", "--agent", "review"]

[agents.codex]
args = ["-c", "model_reasoning_effort=high", "--title", "two words"]
"#;

/// Pull request #12 of `penso/app`, listed from its remote `upstream`.
pub(super) fn pull_request() -> Item {
    Item {
        key: ItemKey {
            source: SourceKey {
                provider: Provider::Github,
                host: "github.com".into(),
                repository: "penso/app".into(),
            },
            native_id: "pr/12".into(),
        },
        identifier: "#12".into(),
        title: "Clarify headings".into(),
        description: None,
        state: "open".into(),
        url: None,
        author: Some("penso".into()),
        labels: Vec::new(),
        parent_id: None,
        blocked_by: Vec::new(),
        priority: None,
        created_at: None,
        updated_at: None,
        pull_request: Some(PullRequest {
            number: 12,
            additions: None,
            deletions: None,
            base_ref: "main".into(),
            head_ref: "headings".into(),
            base_sha: "a".repeat(40),
            head_sha: "b".repeat(40),
            head_repository: Some("penso/app".into()),
        }),
        activity: Some(Activity::default()),
    }
}

/// The config's arguments, and pull request #12 listed from `upstream`.
fn listed(view: &mut OrchestratorView) {
    let config: crate::config::OrchestratorConfig = toml::from_str(CONFIG).unwrap();
    view.set_agent_config(&config);
    let snapshot = &mut view.repo_mut().snapshot;
    let item = pull_request();
    snapshot.repo = Some(RepoInfo {
        repository: Repository::Local("/r/.git".into()),
        main_root: "/r".into(),
        beads: false,
        remote: Some(Remote {
            name: "upstream".into(),
            url: "https://github.com/penso/app.git".into(),
            source: item.key.source.clone(),
        }),
    });
    snapshot.items = Arc::new(vec![item]);
    snapshot.installed_endpoint = None;
    view.refresh_rows();
}

/// The dialog on pull request #12, with opencode and codex installed here.
pub(super) fn open(view: &Entity<OrchestratorView>, cx: &mut VisualTestContext, review: bool) {
    let key = pull_request().key.canonical();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            listed(view);
            if review {
                view.open_review(key, window, cx);
            } else {
                view.open_dispatch(key, window, cx);
            }
            // Opening asks the host for its agents again; this answers.
            view.repo_mut().snapshot.installed =
                Some(Arc::new(vec![AgentKind::Opencode, AgentKind::Codex]));
            view.reconcile_agents(cx);
        })
    });
}

fn shown(view: &Entity<OrchestratorView>, cx: &mut VisualTestContext) -> String {
    cx.update(|_, cx| {
        let view = view.read(cx);
        let dialog = view.dialog.as_ref().unwrap();
        dialog.args.read(cx).text().to_owned()
    })
}

fn type_args(view: &Entity<OrchestratorView>, cx: &mut VisualTestContext, text: &str) {
    cx.update(|_, cx| {
        let input = view.read(cx).dialog.as_ref().unwrap().args.clone();
        input.update(cx, |input, cx| input.set_text_at(text, text.len(), cx));
    });
}

fn pick(view: &Entity<OrchestratorView>, cx: &mut VisualTestContext, kind: AgentKind) {
    cx.update(|_, cx| view.update(cx, |view, cx| view.pick_agent(Some(kind), cx)));
}

#[gpui::test]
fn a_review_opens_on_the_agents_review_arguments_and_a_dispatch_on_its_own(
    cx: &mut TestAppContext,
) {
    let (view, cx, _dir) = view_in_window(cx);
    open(&view, cx, true);
    assert_eq!(shown(&view, cx), "--standalone --agent review");
    open(&view, cx, false);
    assert_eq!(shown(&view, cx), "--standalone");
    // Codex has no review_args, so a review uses its args, quoted to read
    // back as the same words.
    open(&view, cx, true);
    pick(&view, cx, AgentKind::Codex);
    let codex = shown(&view, cx);
    assert_eq!(
        codex,
        "-c 'model_reasoning_effort=high' --title 'two words'"
    );
    assert_eq!(
        shlex::split(&codex).unwrap(),
        ["-c", "model_reasoning_effort=high", "--title", "two words"]
    );
}

#[gpui::test]
fn switching_agents_keeps_what_was_typed_for_each(cx: &mut TestAppContext) {
    let (view, cx, _dir) = view_in_window(cx);
    open(&view, cx, true);
    type_args(&view, cx, "--agent build");
    pick(&view, cx, AgentKind::Codex);
    assert!(shown(&view, cx).starts_with("-c "));
    type_args(&view, cx, "--sandbox read-only");
    pick(&view, cx, AgentKind::Opencode);
    assert_eq!(shown(&view, cx), "--agent build");
    pick(&view, cx, AgentKind::Codex);
    assert_eq!(shown(&view, cx), "--sandbox read-only");
    // Picking the agent already shown changes nothing.
    pick(&view, cx, AgentKind::Codex);
    assert_eq!(shown(&view, cx), "--sandbox read-only");
}

#[gpui::test]
fn the_request_carries_the_arguments_split_as_a_shell_would(cx: &mut TestAppContext) {
    let (view, cx, _dir) = view_in_window(cx);
    open(&view, cx, true);
    type_args(
        &view,
        cx,
        r#"--agent review --title "Review #12" '$HOME' a\ b"#,
    );
    let ready = cx.update(|_, cx| view.update(cx, |view, cx| view.dispatch_request(cx)));
    let ready = ready.unwrap();
    assert_eq!(ready.request.kind, AgentKind::Opencode);
    assert_eq!(
        ready.request.args,
        ["--agent", "review", "--title", "Review #12", "$HOME", "a b"]
    );
    assert_eq!(
        ready.typed_args,
        r#"--agent review --title "Review #12" '$HOME' a\ b"#
    );

    type_args(&view, cx, "");
    let ready = cx.update(|_, cx| view.update(cx, |view, cx| view.dispatch_request(cx)));
    assert!(ready.unwrap().request.args.is_empty());
}

#[gpui::test]
fn arguments_that_cannot_be_split_or_are_too_many_are_refused(cx: &mut TestAppContext) {
    let (view, cx, _dir) = view_in_window(cx);
    open(&view, cx, false);
    for (text, problem) in [
        (
            "--title 'never closed",
            "An argument has a quote that is never closed.",
        ),
        (
            &"-v ".repeat(33),
            "Arguments are limited to 32, of at most 1 KiB each.",
        ),
    ] {
        type_args(&view, cx, text);
        let refused = cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                let ready = view.dispatch_request(cx);
                (ready.is_none(), view.dialog.as_ref().unwrap().problem)
            })
        });
        assert_eq!(refused, (true, Some(problem)), "{text}");
    }
}

#[gpui::test]
fn the_next_dialog_opens_on_the_arguments_last_dispatched(cx: &mut TestAppContext) {
    let (view, cx, _dir) = view_in_window(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.remember_args(AgentKind::Opencode, true, "--agent security".into());
        })
    });
    open(&view, cx, true);
    assert_eq!(shown(&view, cx), "--agent security");
    // A dispatch, not a review, still opens on the config's.
    open(&view, cx, false);
    assert_eq!(shown(&view, cx), "--standalone");
}
