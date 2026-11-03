use {
    super::*,
    crate::{close_modal::CloseConfirmation, window::MockPeer},
    sheprd_client::protocol::AgentStatus,
};

// These sequence tests drive their own projection; rendering the terminal
// would resize it and make that projection unready between confirmations.
struct Fixture(Entity<SheprdWindow>);

impl Render for Fixture {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

/// A split whose left group lists four Herdr tabs, each with an agent at
/// work so its close asks first, and shows "t0".
fn busy_split(
    cx: &mut TestAppContext,
) -> (
    Entity<SheprdWindow>,
    &mut VisualTestContext,
    GroupId,
    MockPeer,
) {
    let peer = MockPeer::new();
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| {
            let mut view = fixture_window(window, cx);
            peer.prepare(&mut view, cx);
            let mut shown = snapshot(40);
            shown.focused_workspace_id = Some("w0".into());
            shown.focused_tab_id = Some("t0".into());
            let surface = Arc::make_mut(view.live.surface.as_mut().unwrap());
            surface.boot_id = shown.boot_id.clone();
            surface.projection_revision = shown.revision;
            view.live.snapshot = Some(Arc::new(shown));
            view
        }))
    });
    let view = fixture.read_with(cx, |fixture, _| fixture.0.clone());
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
            for id in ["x", "y"] {
                let mut tab = snapshot.tabs[0].clone();
                tab.tab_id = id.into();
                tab.workspace_id = "w0".into();
                snapshot.tabs.push(tab);
            }
            for tab in &mut snapshot.tabs {
                tab.agent_status = AgentStatus::Working;
            }
            view.command(Command::SplitEditor, window, cx);
        })
    });
    let left = groups(&view, cx)[0];
    (view, cx, left, peer)
}

fn herdr(ids: &[&str]) -> Vec<Pick> {
    ids.iter().map(|id| Pick::Herdr((*id).into())).collect()
}

/// The Herdr tab the close confirmation is open for, of `candidates`.
fn asking(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) -> Option<&'static str> {
    view.read_with(cx, |view, _| {
        if view.menu.page != Some(Page::ConfirmClose) {
            return None;
        }
        let snapshot = view.live.snapshot.as_ref().unwrap();
        ["t0", "t1", "x", "y"]
            .into_iter()
            .find(|tab| view.menu.close == CloseConfirmation::capture_tab(snapshot, tab))
    })
}

fn tick(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) {
    cx.update(|window, cx| view.update(cx, |view, cx| view.advance_close_run(window, cx)));
    draw(cx);
}

fn decline(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) {
    cx.update(|window, cx| view.update(cx, |view, cx| view.dismiss_menu(window, cx)));
    tick(view, cx);
}

/// The user accepts the dialog and the daemon closes `tab`.
fn accept(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext, tab: &str) {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.close_sent(tab);
            view.dismiss_menu(window, cx);
        })
    });
    // Until the daemon answers, the next tab is not asked about.
    tick(view, cx);
    assert_eq!(asking(view, cx), None);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
            snapshot.tabs.retain(|candidate| candidate.tab_id != tab);
        })
    });
    tick(view, cx);
}

#[gpui::test]
fn close_others_closes_pages_at_once_and_keeps_the_tab_shown(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    open_pages(&view, cx, 3);
    let pages = cx.update(|_, cx| view.read(cx).browser_tab_ids(cx));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.command(Command::SplitEditor, window, cx)
        })
    });
    let [left, right] = groups(&view, cx)[..] else {
        panic!("two groups")
    };
    let herdr_before = tabs(&view, cx, left).len() - pages.len();
    for page in &pages {
        move_page(&view, cx, right, *page);
    }
    assert!(actions(&view, cx, right).contains(&Action::CloseOthers));
    run(&view, cx, right, Action::CloseOthers);
    // Only the group's own other tabs closed, with nothing to confirm.
    assert_eq!(tabs(&view, cx, right), [Pick::Page(pages[2])]);
    assert_eq!(tabs(&view, cx, left).len(), herdr_before);
    view.read_with(cx, |view, _| {
        assert!(view.menu.page.is_none());
        assert!(view.browser.closing.is_none());
    });
    // With one tab, there are no others to close.
    assert!(!actions(&view, cx, right).contains(&Action::CloseOthers));
}

#[gpui::test]
fn close_others_asks_about_each_herdr_tab_in_turn(cx: &mut TestAppContext) {
    let (view, cx, left, _peer) = busy_split(cx);
    assert_eq!(tabs(&view, cx, left), herdr(&["t0", "t1", "x", "y"]));
    run(&view, cx, left, Action::CloseOthers);
    // The tab the group shows is never asked about; the others are, in
    // strip order, one dialog at a time.
    assert_eq!(asking(&view, cx), Some("t1"));
    tick(&view, cx);
    assert_eq!(asking(&view, cx), Some("t1"));
    accept(&view, cx, "t1");
    assert_eq!(asking(&view, cx), Some("x"));
    accept(&view, cx, "x");
    assert_eq!(asking(&view, cx), Some("y"));
    accept(&view, cx, "y");
    assert_eq!(asking(&view, cx), None);
    assert_eq!(tabs(&view, cx, left), herdr(&["t0"]));
    view.read_with(cx, |view, _| assert!(view.browser.closing.is_none()));
}

#[gpui::test]
fn declining_one_close_leaves_the_rest_open(cx: &mut TestAppContext) {
    let (view, cx, left, _peer) = busy_split(cx);
    run(&view, cx, left, Action::CloseOthers);
    accept(&view, cx, "t1");
    assert_eq!(asking(&view, cx), Some("x"));
    decline(&view, cx);
    // Nothing more is asked or closed.
    assert_eq!(asking(&view, cx), None);
    tick(&view, cx);
    assert_eq!(asking(&view, cx), None);
    assert_eq!(tabs(&view, cx, left), herdr(&["t0", "x", "y"]));
    view.read_with(cx, |view, _| assert!(view.browser.closing.is_none()));
}
