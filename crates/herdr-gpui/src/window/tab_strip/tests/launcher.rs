use super::*;
use crate::{
    chat::Program,
    config::ChatLauncher,
    window::{
        MockPeer,
        tab_strip::launcher::{BUTTON, GAP, RULE, card_left, spread},
    },
};
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

/// Whether `view`'s first strip deals its deck out, rather than gathering
/// it back, or `None` before the pointer ever came.
fn dealing(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) -> Option<bool> {
    view.read_with(cx, |view, _| {
        view.chats.launcher_deck.map(|(_, deck)| deck.is_open())
    })
}

#[gpui::test]
fn the_default_launcher_deals_the_agents_out_of_plus_and_gathers_them_back(
    cx: &mut TestAppContext,
) {
    let (view, cx, _peer) = strip(cx, ChatLauncher::default());
    assert!(shown(cx, "new-tab"));
    assert!(
        !shown(cx, CLAUDE),
        "stacked under + until the pointer comes"
    );
    assert_eq!(dealing(&view, cx), None);
    hover(cx, "new-tab");
    assert_eq!(dealing(&view, cx), Some(true));
    cx.simulate_mouse_move(point(px(800.), px(400.)), None, Modifiers::none());
    draw(cx);
    assert_eq!(
        dealing(&view, cx),
        Some(false),
        "gathering back, not gone at once"
    );

    // Once dealt, each card stands in its place after "+" and the rule.
    let group = view.read_with(cx, |view, _| view.group_slots()[0].id);
    view.update(cx, |view, _| {
        let mut deck = crate::motion::Toggle::default();
        deck.set(true, crate::motion::now(), true);
        view.chats.launcher_deck = Some((group, deck));
    });
    draw(cx);
    let plus = cx.debug_bounds("new-tab").unwrap();
    let deck = cx.debug_bounds("chat-launcher-deck").unwrap();
    let claude = cx.debug_bounds(CLAUDE).unwrap();
    let codex = cx.debug_bounds(CODEX).unwrap();
    assert!(
        (deck.left() - plus.right()).abs() <= px(0.5),
        "{deck:?} after {plus:?}"
    );
    assert_eq!(deck.size.width, px(RULE + 2. * BUTTON + GAP));
    assert_eq!(claude.left(), deck.left() + px(RULE));
    assert_eq!(codex.left(), claude.right() + px(GAP));
}

#[test]
fn the_deck_deals_its_cards_one_after_another_from_under_plus() {
    let count = 3;
    for index in 0..count {
        assert_eq!(spread(0., index, count), 0.);
        assert_eq!(spread(1., index, count), 1.);
        assert_eq!(card_left(0., index), -BUTTON, "hidden under +");
        assert_eq!(card_left(1., index), RULE + (BUTTON + GAP) * index as f32);
    }
    // Midway the first card has gone further than the last, and none moves
    // back as the deal goes on.
    assert!(spread(0.4, 0, count) > spread(0.4, count - 1, count));
    let mut last = vec![0.; count];
    for step in 0..=20 {
        let shown = step as f32 / 20.;
        for (index, last) in last.iter_mut().enumerate() {
            let t = spread(shown, index, count);
            assert!(t >= *last);
            *last = t;
        }
    }
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
