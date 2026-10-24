use super::*;
use crate::{
    chat::Program,
    chat::Toward,
    config::{ChatLauncher, TabLayoutMode},
    window::{
        MockPeer,
        tab_strip::launcher::{card_offset, deal_toward, spread},
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

/// Deals `view`'s first strip's deck all the way out toward `toward`.
fn deal(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext, toward: Toward) {
    view.update(cx, |view, _| {
        let group = view.group_slots()[0].id;
        let mut toggle = crate::motion::Toggle::default();
        toggle.set(true, crate::motion::now(), true);
        view.chats.launcher_deck = Some(crate::chat::LauncherDeck {
            group,
            over_cards: false,
            over_plus: true,
            toggle,
            toward,
        });
    });
    draw(cx);
}

fn shown(cx: &mut VisualTestContext, selector: &'static str) -> bool {
    cx.debug_bounds(selector).is_some()
}

/// Whether `view`'s first strip deals its deck out, rather than gathering
/// it back, or `None` before the pointer ever came.
fn dealing(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) -> Option<bool> {
    view.read_with(cx, |view, _| {
        view.chats.launcher_deck.map(|deck| deck.toggle.is_open())
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

    // Once dealt, each card stands in its place after "+", shaped like it.
    let before = cx.debug_bounds("new-tab").unwrap();
    deal(&view, cx, Toward::Right);
    let plus = cx.debug_bounds("new-tab").unwrap();
    // Dealt over what follows "+", so "+" stays under the pointer.
    assert_eq!(plus, before, "+ never moves as the deck deals");
    let deck = cx.debug_bounds("chat-launcher-deck").unwrap();
    let claude = cx.debug_bounds(CLAUDE).unwrap();
    let codex = cx.debug_bounds(CODEX).unwrap();
    assert!(
        (deck.left() - plus.right()).abs() <= px(0.5),
        "{deck:?} after {plus:?}"
    );
    assert_eq!(deck.size.width, plus.size.width * 2.);
    assert_eq!(claude.left(), deck.left());
    assert_eq!(codex.left(), claude.right());
    assert_eq!(claude.size, plus.size);
}

/// With no room for the cards past "+", as in the rightmost group when the
/// tabs fill it, the deck deals to the left of "+", over the tabs.
#[gpui::test]
fn the_deck_deals_left_where_the_window_ends(cx: &mut TestAppContext) {
    let (view, cx, _peer) = strip(cx, ChatLauncher::default());
    let plus = cx.debug_bounds("new-tab").unwrap();
    deal(&view, cx, Toward::Left);
    let claude = cx.debug_bounds(CLAUDE).unwrap();
    let codex = cx.debug_bounds(CODEX).unwrap();
    assert_eq!(cx.debug_bounds("new-tab").unwrap(), plus);
    assert_eq!(claude.right(), plus.left(), "the first card beside +");
    assert_eq!(codex.right(), claude.left());

    // Hovered with room past "+" for both cards, it picks the right.
    view.update(cx, |view, _| view.chats.launcher_deck = None);
    hover(cx, "new-tab");
    let toward = view.read_with(cx, |view, _| {
        view.chats.launcher_deck.map(|deck| deck.toward)
    });
    assert_eq!(toward, Some(Toward::Right));
}

#[test]
fn the_deck_deals_right_only_while_every_card_fits_there() {
    // From the middle of "+": its own far half, then each card.
    assert_eq!(deal_toward(17. + 3. * 34., 34., 3), Toward::Right);
    assert_eq!(deal_toward(17. + 3. * 34. - 1., 34., 3), Toward::Left);
    assert_eq!(
        deal_toward(65., 30., 3),
        Toward::Left,
        "the third card cut off"
    );
}

/// When tabs fill the strip, a deck that took room beside "+" would squeeze
/// them and slide "+" away from the pointer, so a click meant for "+" could
/// start a chat. Moving from "+" onto a card keeps the deck dealt.
#[gpui::test]
fn the_deck_keeps_plus_in_place_and_stays_dealt_over_its_cards(cx: &mut TestAppContext) {
    let (view, cx, _peer) = strip(cx, ChatLauncher::default());
    cx.simulate_resize(size(px(420.), px(600.)));
    draw(cx);
    let before = cx.debug_bounds("new-tab").unwrap();
    hover(cx, "new-tab");
    // Let the deal finish, so the cards stand where the pointer goes next.
    view.update(cx, |view, _| {
        if let Some(deck) = &mut view.chats.launcher_deck {
            deck.toggle.set(true, crate::motion::now(), true);
        }
    });
    draw(cx);
    assert_eq!(cx.debug_bounds("new-tab").unwrap(), before);
    hover(cx, CODEX);
    assert_eq!(dealing(&view, cx), Some(true), "still dealt over a card");
    let codex = cx.debug_bounds(CODEX).unwrap();
    assert!(codex.left() >= before.right());
}

/// Every layout draws its own "+", and the agents beside it, dealt or
/// standing, take that same shape: classic's full-height cells, the
/// floating layouts' small rounded buttons.
#[gpui::test]
fn the_agents_take_each_layouts_own_plus(cx: &mut TestAppContext) {
    for &mode in TabLayoutMode::ALL {
        let (view, cx, _peer) = strip(cx, ChatLauncher::Off);
        view.update(cx, |view, _| view.config.tab_layout = mode);
        draw(cx);
        let alone = cx.debug_bounds("new-tab").unwrap();
        let width = view.read_with(cx, |view, _| view.tab_layout().new_tab_width());
        assert_eq!(alone.size.width, px(width), "{mode:?}");
        for launcher in [ChatLauncher::Plus, ChatLauncher::Beside] {
            view.update(cx, |view, _| view.config.chat.launcher = launcher);
            draw(cx);
            deal(&view, cx, Toward::Right);
            let plus = cx.debug_bounds("new-tab").unwrap();
            assert_eq!(plus, alone, "{mode:?} {launcher:?}");
            let claude = cx.debug_bounds(CLAUDE).unwrap();
            assert_eq!(claude.size, plus.size, "{mode:?} {launcher:?}");
            assert_eq!(claude.top(), plus.top(), "{mode:?} {launcher:?}");
        }
    }
}

#[test]
fn the_deck_deals_its_cards_one_after_another_from_under_plus() {
    let count = 3;
    let width = 34.;
    for index in 0..count {
        assert_eq!(spread(0., index, count), 0.);
        assert_eq!(spread(1., index, count), 1.);
        assert_eq!(card_offset(0., index, width), -width, "hidden under +");
        assert_eq!(card_offset(1., index, width), width * index as f32);
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
