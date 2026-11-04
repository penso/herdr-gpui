use super::*;
use crate::config::SelectMode;
use std::collections::BTreeMap;

/// A local host beside a remote one named `Remote`, whose workspace is
/// `remote workspace` and whose agent pane is `r0`, so each host's rows can
/// be told apart by probe. Returns the selectors that carried a wash.
fn washed(
    cx: &mut gpui::TestAppContext,
    mode: crate::config::LayoutMode,
    select: SelectMode,
    hosts: &[(&str, u32)],
    selected_endpoint: usize,
) -> Vec<&'static str> {
    let hosts: BTreeMap<String, u32> = hosts
        .iter()
        .map(|(name, color)| ((*name).to_owned(), *color))
        .collect();
    let (_fixture, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut view = fixture_window(window, cx);
            view.config.layout.mode = mode;
            view.config.sidebar_style.select = select;
            view.config.sidebar_style.hosts = hosts;
            let mut remote = crate::endpoint::Endpoint::new(
                "ssh:test".into(),
                "Remote".into(),
                ConnectTarget::Ssh {
                    target: "unused".into(),
                    session: "default".into(),
                },
                true,
            );
            remote.live.snapshot = view.live.snapshot.clone();
            // The selected host draws the window's own live snapshot, so the
            // remote's names go wherever the remote will read them from.
            let snapshot = if selected_endpoint == 1 {
                &mut view.live.snapshot
            } else {
                &mut remote.live.snapshot
            };
            let snapshot = Arc::make_mut(snapshot.as_mut().unwrap());
            snapshot.workspaces[0].label = "remote workspace".into();
            snapshot.agents[0].pane_id = "r0".into();
            view.endpoints.push(remote);
            view.selected_endpoint = selected_endpoint;
            view
        });
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        SidebarFixture(view)
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.run_until_parked();
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    [
        "wash-host-local",
        "wash-herdr",
        "wash-agent-p0",
        "wash-host-ssh:test",
        "wash-remote workspace",
        "wash-agent-r0",
    ]
    .into_iter()
    .filter(|selector| cx.debug_bounds(selector).is_some())
    .collect()
}

const REMOTE_ROWS: [&str; 3] = [
    "wash-host-ssh:test",
    "wash-remote workspace",
    "wash-agent-r0",
];
const LOCAL_ROWS: [&str; 3] = ["wash-host-local", "wash-herdr", "wash-agent-p0"];

/// A configured colour washes the host's header, its workspace rows, and
/// its agents' rows, and no one else's, in every layout.
#[gpui::test]
fn a_host_colour_washes_only_that_hosts_rows(cx: &mut gpui::TestAppContext) {
    for mode in crate::config::LayoutMode::ALL
        .into_iter()
        .filter(|mode| mode.lists_workspaces())
    {
        assert_eq!(
            washed(cx, mode, SelectMode::Row, &[("Remote", 0x336699)], 0),
            REMOTE_ROWS,
            "{mode}"
        );
        assert!(
            washed(cx, mode, SelectMode::Row, &[], 0).is_empty(),
            "{mode}: nothing configured, nothing washed"
        );
    }
}

/// In the group modes the selected host is always washed, with or without a
/// colour of its own; the other host keeps only its configured colour.
#[gpui::test]
fn group_modes_wash_the_selected_host(cx: &mut gpui::TestAppContext) {
    let mode = crate::config::LayoutMode::default();
    for select in [SelectMode::Group, SelectMode::GroupDim] {
        // Local selected, no colour anywhere: only local is washed.
        assert_eq!(washed(cx, mode, select, &[], 0), LOCAL_ROWS, "{select:?}");
        // Remote selected and coloured, local uncoloured: only remote.
        assert_eq!(
            washed(cx, mode, select, &[("Remote", 0x336699)], 1),
            REMOTE_ROWS,
            "{select:?}"
        );
        // Local selected while remote is coloured: both show.
        let mut both = LOCAL_ROWS.to_vec();
        both.extend(REMOTE_ROWS);
        assert_eq!(
            washed(cx, mode, select, &[("Remote", 0x336699)], 0),
            both,
            "{select:?}"
        );
    }
}

/// The wash is a layer of the row, the size of its highlight.
#[gpui::test]
fn the_wash_is_the_size_of_the_highlight(cx: &mut gpui::TestAppContext) {
    let (_fixture, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut view = fixture_window(window, cx);
            view.config.sidebar_style.hosts =
                BTreeMap::from([(view.endpoints[0].label.clone(), 0x336699)]);
            view
        });
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        SidebarFixture(view)
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.run_until_parked();
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    let wash = cx.debug_bounds("wash-herdr").unwrap();
    let highlight = cx.debug_bounds("highlight-herdr").unwrap();
    let row = cx.debug_bounds("row-herdr").unwrap();
    assert_eq!(wash, highlight, "the wash covers exactly the highlight");
    assert!(row.contains(&wash.center()));
}

/// The solid layers painted exactly over the highlight `selector`, bottom
/// first: the highlight's own fill and the host's wash, in paint order.
fn layers(cx: &mut gpui::VisualTestContext, selector: &'static str) -> Vec<gpui::Background> {
    let region = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} is drawn"));
    cx.update(|window, _| {
        let region = region.scale(window.scale_factor());
        let mut quads = window.painted_quads();
        quads.sort_by_key(|quad| quad.order);
        quads
            .into_iter()
            .filter(|quad| quad.bounds == region)
            .map(|quad| quad.background)
            .filter(|background| background.as_solid().is_some_and(|color| color.a > 0.))
            .collect()
    })
}

/// A selected row keeps its host's colour in flat layouts as it does in
/// rounded ones. A flat fill is opaque, so the wash paints over it; a
/// rounded outline is translucent, so the wash stays under it.
#[gpui::test]
fn a_selected_row_keeps_its_host_colour(cx: &mut gpui::TestAppContext) {
    use crate::config::{Density, LayoutMode, Style};
    use gpui::{Background, Hsla, rgb, rgba};

    const COLOUR: u32 = 0x336699;
    let wash = Background::from(rgba((COLOUR << 8) | super::super::wash::REST_ALPHA));
    for style in [Style::Flat, Style::Rounded] {
        let (fixture, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|cx| {
                let mut view = fixture_window(window, cx);
                view.config.layout.mode = LayoutMode::new(Density::Comfortable, style);
                view.config.sidebar_style.hosts =
                    BTreeMap::from([(view.endpoints[0].label.clone(), COLOUR)]);
                // A second host, so the selected one draws a header.
                view.endpoints.push(crate::endpoint::Endpoint::new(
                    "ssh:test".into(),
                    "Remote".into(),
                    ConnectTarget::Ssh {
                        target: "unused".into(),
                        session: "default".into(),
                    },
                    true,
                ));
                view
            });
            cx.observe(&view, |_, _, cx| cx.notify()).detach();
            SidebarFixture(view)
        });
        cx.simulate_resize(size(px(800.), px(600.)));
        cx.run_until_parked();
        cx.update(|window, cx| full_draw(window, cx).clear(cx));
        let theme = cx.update(|_, cx| fixture.read(cx).0.read(cx).theme.clone());
        let selected = match style {
            Style::Flat => Background::from(Hsla::from(rgb(theme.active))),
            Style::Rounded => Background::from(rgba((theme.foreground << 8) | 0x14)),
        };
        let expected = match style {
            Style::Flat => [selected, wash],
            Style::Rounded => [wash, selected],
        };
        // The focused workspace and the header of the host it is on.
        for selector in ["highlight-herdr", "highlight-host-local"] {
            assert_eq!(layers(cx, selector), expected, "{style:?} {selector}");
        }
    }
}
