use super::*;
use crate::hub::{Error as HubError, Workspace};
use sheprd_chat_protocol::{Agent, ENDPOINT_ENV, EndpointId, TOKEN};
use std::path::PathBuf;

fn chat_record(address: &str, endpoint: &EndpointId) -> Value {
    let mut record = record(address);
    record["labels"] = json!({TOKEN: endpoint.to_string()});
    record
}

/// A hub whose Herdr 1 socket answers nothing, so reaching it fails with
/// `NotConnected` and a fallback cannot pass for success.
fn hub_on(source: &Arc<Source>, layout: Layout) -> Hub {
    let mut hub = Hub::offline_in(layout, None, None);
    hub.core = Some(source.clone());
    hub.program = Some(PathBuf::from("/bin/sheprd-acp"));
    hub
}

fn workspace(id: &str, cwd: &str) -> Workspace {
    Workspace {
        branch: None,
        chats: Vec::new(),
        cwd: cwd.to_owned(),
        id: id.to_owned(),
        label: id.to_owned(),
        terminals: Vec::new(),
        worktree: None,
    }
}

fn request(fake: &Fake) -> Value {
    let bytes = fake.inputs.recv_timeout(Duration::from_secs(2)).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn fake_with_chat(endpoint: &EndpointId) -> (Arc<Source>, Fake) {
    let records = vec![record("t1"), chat_record("t9", endpoint)];
    fake_with_doc(false, Value::Null, records)
}

#[test]
fn a_labelled_terminal_is_listed_as_a_chat_and_not_as_a_terminal() {
    let endpoint = EndpointId::random();
    let (source, fake) = fake_with_chat(&endpoint);
    let layout = source.poll().unwrap();
    let chat = layout.chat(&id("t9")).unwrap().1;
    assert_eq!(chat.endpoint, endpoint);
    assert_eq!(chat.agent.as_deref(), Some("claude"));
    assert!(layout.terminal(&id("t9")).is_none());
    assert!(layout.terminal(&id("t1")).is_some());
    drop(source);
    fake.server.join().unwrap();
}

#[test]
fn a_chat_on_another_machine_stays_a_terminal() {
    let endpoint = EndpointId::random();
    let records = vec![chat_record("box/t9", &endpoint)];
    let (source, fake) = fake_with_doc(false, Value::Null, records);
    let layout = source.poll().unwrap();
    assert!(layout.chat(&id("box/t9")).is_none());
    assert!(layout.terminal(&id("box/t9")).is_some());
    drop(source);
    fake.server.join().unwrap();
}

#[test]
fn merging_keeps_core_chats_in_a_workspace_both_sources_share() {
    let endpoint = EndpointId::random();
    let (source, fake) = fake_with_chat(&endpoint);
    let core = source.poll().unwrap();
    let daemon = Layout {
        workspaces: vec![workspace(UNLISTED, "/code")],
        ..Layout::default()
    };
    assert!(merge(&daemon, &core).chat(&id("t9")).is_some());
    drop(source);
    fake.server.join().unwrap();
}

#[tokio::test]
async fn a_chat_starts_on_the_core_with_its_endpoint_labelled() {
    let (source, fake) = fake_with_chat(&EndpointId::random());
    source.poll().unwrap();
    let layout = Layout {
        connected: true,
        workspaces: vec![workspace("w1", "/code/one")],
        ..Layout::default()
    };
    let hub = hub_on(&source, layout);
    let pane = hub.start_chat("w1", Agent::Claude, None).await.unwrap();
    assert_eq!(pane, id("t9"));
    let run = request(&fake);
    assert_eq!(run["method"], "terminal.run");
    let params = &run["params"];
    assert_eq!(params["argv"], json!(["/bin/sheprd-acp", "claude"]));
    assert_eq!(params["cwd"], "/code/one");
    assert_eq!(params["env"][ENDPOINT_ENV], params["labels"][TOKEN]);
    assert_eq!(params["env"]["HERDR_PANE_ID"], "");
    drop((hub, source));
    fake.server.join().unwrap();
}

#[tokio::test]
async fn a_core_failure_is_reported_and_not_retried_on_herdr_one() {
    let (source, fake) = fake();
    source.poll().unwrap();
    let hub = hub_on(&source, Layout::default());
    let error = hub
        .create_workspace("/refused", None, Agent::Claude)
        .await
        .err()
        .unwrap();
    assert!(matches!(error, HubError::Core(Error::Client(_))), "{error}");
    drop((hub, source));
    fake.server.join().unwrap();
}

#[tokio::test]
async fn a_worktree_is_refused_while_a_core_answers() {
    let (source, fake) = fake();
    source.poll().unwrap();
    let hub = hub_on(&source, Layout::default());
    let error = hub
        .create_workspace("/code", Some("topic"), Agent::Claude)
        .await
        .err()
        .unwrap();
    assert!(
        matches!(error, HubError::Core(Error::Unsupported)),
        "{error}"
    );
    drop((hub, source));
    fake.server.join().unwrap();
}

#[tokio::test]
async fn without_a_core_new_work_stays_with_herdr_one() {
    let (source, fake) = fake();
    let hub = hub_on(&source, Layout::default());
    assert!(hub.new_core_tab("w1").await.unwrap().is_none());
    drop((hub, source));
    fake.server.join().unwrap();
}

#[tokio::test]
async fn a_new_tab_opens_a_shell_in_the_workspace_folder_and_answers_its_pane() {
    let records = vec![record("t1"), record("t9")];
    let (source, fake) = fake_with_doc(false, Value::Null, records);
    let mut layout = source.poll().unwrap();
    layout.workspaces.push(workspace("w1", "/code/one"));
    let hub = hub_on(&source, layout);
    let answer = hub.new_core_tab("w1").await.unwrap().unwrap();
    assert_eq!(answer["pane"]["pane_id"], id("t9"));
    let run = request(&fake);
    assert_eq!(run["params"]["cwd"], "/code/one");
    assert_eq!(run["params"]["keep"], true);
    assert!(run["params"].get("argv").is_none());
    drop((hub, source));
    fake.server.join().unwrap();
}

#[test]
fn closing_ends_one_named_terminal_and_refuses_a_tab_of_several() {
    let (source, fake) = fake();
    source.poll().unwrap();
    source
        .call(ApiMethod::PaneClose, &json!({"pane_id": id("t1")}))
        .unwrap();
    let kill = request(&fake);
    assert_eq!(kill["method"], "terminal.kill");
    assert_eq!(kill["params"]["terminal"], "t1");
    let tab = json!({"tab_id": "core:app-tab:ab"});
    assert!(matches!(
        source.call(ApiMethod::TabClose, &tab).unwrap_err(),
        HubError::Core(Error::Unsupported)
    ));
    drop(source);
    fake.server.join().unwrap();
}
