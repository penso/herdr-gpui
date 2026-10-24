use super::*;
use crate::{chat::Program, config::ChatLauncher, window::MockPeer};
use herdr_chat_protocol::Agent;

const CLAUDE: &str = "chat-launch-claude";
const CODEX: &str = "chat-launch-codex";

/// A window on this user's local daemon that can start chats, with Claude
/// and Codex installed, its launcher placed as `launcher` says.
fn strip(
    cx: &mut TestAppContext,
    launcher: ChatLauncher,
) -> (Entity<HerdrWindow>, &mut VisualTestContext, MockPeer) {
    let peer = MockPeer::advertising(&["tab.create"]);
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            crate::worktree_scripts::tests::connect(view, &peer, cx);
            view.config.features.acp_chats = true;
            view.config.chat.launcher = launcher;
            view.endpoints[0].connection.target = herdr_client::ConnectTarget::Local;
            view.live.local_daemon_peer = true;
            view.endpoints[0].live = view.live.clone();
            view.chats.program = Program::Found("/opt/herdr-acp".into());
            let id = view.endpoints[0].id.clone();
            view.chats
                .agents
                .record(&id, vec![Agent::Claude, Agent::Codex]);
        })
    });
    cx.simulate_resize(size(px(1600.), px(600.)));
    draw(cx);
    (view, cx, peer)
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
}

fn hover(cx: &mut VisualTestContext, selector: &'static str) {
    let at = cx.debug_bounds(selector).unwrap().center();
    cx.simulate_mouse_move(at, None, Modifiers::none());
    draw(cx);
}

fn shown(cx: &mut VisualTestContext, selector: &'static str) -> bool {
    cx.debug_bounds(selector).is_some()
}

#[gpui::test]
fn the_default_launcher_shows_the_agents_beside_plus_under_the_pointer(cx: &mut TestAppContext) {
    let (_view, cx, _peer) = strip(cx, ChatLauncher::default());
    assert!(shown(cx, "new-tab"));
    assert!(!shown(cx, CLAUDE), "hidden until the pointer comes");
    hover(cx, "new-tab");
    let plus = cx.debug_bounds("new-tab").unwrap();
    let claude = cx.debug_bounds(CLAUDE).unwrap();
    let codex = cx.debug_bounds(CODEX).unwrap();
    assert!(plus.right() <= claude.left(), "{plus:?} then {claude:?}");
    assert!(claude.right() <= codex.left());
    // Leaving the strip's controls folds them away again.
    cx.simulate_mouse_move(point(px(800.), px(400.)), None, Modifiers::none());
    draw(cx);
    assert!(!shown(cx, CLAUDE));
}

#[gpui::test]
fn each_placement_puts_the_agents_where_it_says(cx: &mut TestAppContext) {
    let (_view, cx, _peer) = strip(cx, ChatLauncher::Beside);
    let plus = cx.debug_bounds("new-tab").unwrap();
    let claude = cx.debug_bounds(CLAUDE).unwrap();
    assert!(plus.right() <= claude.left());
    assert!(claude.left() - plus.right() < px(20.), "right after +");

    let (_view, cx, _peer) = strip(cx, ChatLauncher::Leading);
    let tab = cx.debug_bounds("tab-w1:t1").unwrap();
    let codex = cx.debug_bounds(CODEX).unwrap();
    assert!(codex.right() <= tab.left(), "{codex:?} before {tab:?}");

    let (_view, cx, _peer) = strip(cx, ChatLauncher::Trailing);
    let plus = cx.debug_bounds("new-tab").unwrap();
    let claude = cx.debug_bounds(CLAUDE).unwrap();
    let split = cx.debug_bounds("split-editor").unwrap();
    assert!(claude.left() - plus.right() > px(200.), "at the far end");
    assert!(cx.debug_bounds(CODEX).unwrap().right() <= split.left());

    let (_view, cx, _peer) = strip(cx, ChatLauncher::Off);
    assert!(!shown(cx, CLAUDE));
    hover(cx, "new-tab");
    assert!(!shown(cx, CLAUDE));
}

#[gpui::test]
fn the_one_button_offers_the_agent_last_started(cx: &mut TestAppContext) {
    let (view, cx, _peer) = strip(cx, ChatLauncher::Button);
    assert!(shown(cx, CLAUDE), "the first agent before any start");
    assert!(!shown(cx, CODEX));
    hover(cx, CLAUDE);
    let codex = cx.debug_bounds(CODEX).unwrap();
    let claude = cx.debug_bounds(CLAUDE).unwrap();
    assert!(
        codex.right() <= claude.left(),
        "the others slide out before it"
    );

    view.update(cx, |view, _| view.chats.last_started = Some(Agent::Codex));
    cx.simulate_mouse_move(point(px(800.), px(400.)), None, Modifiers::none());
    draw(cx);
    assert!(shown(cx, CODEX));
    assert!(!shown(cx, CLAUDE));
}

#[gpui::test]
fn no_agent_shows_while_no_chat_can_start(cx: &mut TestAppContext) {
    let (view, cx, _peer) = strip(cx, ChatLauncher::Beside);
    assert!(shown(cx, CLAUDE));
    view.update(cx, |view, _| view.chats.program = Program::Missing);
    draw(cx);
    assert!(!shown(cx, CLAUDE));
    view.update(cx, |view, _| {
        view.chats.program = Program::Found("/opt/herdr-acp".into());
        view.config.features.acp_chats = false;
    });
    draw(cx);
    assert!(!shown(cx, CLAUDE));
}

#[gpui::test]
fn a_click_starts_that_agents_chat_in_the_strips_group(cx: &mut TestAppContext) {
    let (view, cx, mut peer) = strip(cx, ChatLauncher::Beside);
    let group = view.read_with(cx, |view, _| view.group_slots()[0].id);
    let codex = cx.debug_bounds(CODEX).unwrap().center();
    cx.simulate_click(codex, Modifiers::none());
    view.read_with(cx, |view, _| {
        assert!(view.chat_starting());
        assert_eq!(view.chats.last_started, Some(Agent::Codex));
        assert_eq!(view.expected_new_tab_group(), Some(group));
        assert!(view.flash.is_none(), "{:?}", view.flash);
    });
    let request = peer.request();
    assert_eq!(request["method"], "tab.create");
    assert_eq!(request["params"]["label"], "Codex");
}
