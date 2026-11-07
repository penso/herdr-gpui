use super::*;
use core::prelude::v1::test;
use sheprd_client::protocol::{FrameData, PaneSurfacePane};

mod remembered;

/// w1:t1 holds the blocked agent w1:p1; w1:t2 holds the terminal w1:p2.
fn snapshot() -> ClientShellSnapshot {
    let mut snapshot: ClientShellSnapshot = serde_json::from_str(include_str!(
        "../../../../sheprd-protocol/tests/fixtures/endpoint-snapshot-v1.json"
    ))
    .unwrap();
    let mut tab = snapshot.tabs[0].clone();
    tab.tab_id = "w1:t2".into();
    tab.label = "logs".into();
    snapshot.tabs.push(tab);
    let mut pane = snapshot.panes[0].clone();
    pane.pane_id = "w1:p2".into();
    pane.tab_id = "w1:t2".into();
    pane.label = Some("tail".into());
    pane.foreground_cwd = None;
    pane.cwd = Some("/repo/logs".into());
    snapshot.panes.push(pane);
    snapshot
}

const INNER: SurfaceRect = SurfaceRect {
    x: 1,
    y: 1,
    width: 78,
    height: 22,
};

/// What the daemon streams for the agent's tab, w1:t1.
fn surface(boot: &str) -> Arc<PaneSurfaceFrame> {
    Arc::new(PaneSurfaceFrame {
        boot_id: boot.into(),
        projection_revision: 1,
        surface_revision: 1,
        frame: FrameData {
            width: 80,
            height: 24,
            cells: vec![],
            cursor: None,
            hyperlinks: vec![],
            graphics: vec![],
        },
        panes: vec![PaneSurfacePane {
            pane_id: "w1:p1".into(),
            content_revision: 1,
            rect: SurfaceRect {
                x: 0,
                y: 0,
                width: 80,
                height: 24,
            },
            inner_rect: INNER,
            scrollbar_rect: None,
            scroll: None,
            focused: true,
            mouse_reporting: false,
            sgr_pixel_mouse: false,
            alternate_screen_active: false,
            pixel_width: 800,
            pixel_height: 480,
        }],
        splits: vec![],
        popup: None,
        graphics: Default::default(),
    })
}

fn shown(preview: Option<Preview>) -> Option<SurfaceRect> {
    match preview? {
        Preview::Screen { rect, .. } => Some(rect),
        Preview::Facts { .. } => None,
    }
}

fn facts(preview: Option<Preview>) -> (String, Vec<(String, String)>) {
    match preview.unwrap() {
        Preview::Facts { title, facts } => (
            title.to_string(),
            facts
                .into_iter()
                .map(|(name, value)| (name.to_string(), value.to_string()))
                .collect(),
        ),
        Preview::Screen { .. } => panic!("expected facts"),
    }
}

#[test]
fn a_pane_on_the_streamed_tab_shows_its_own_cells() {
    let snapshot = snapshot();
    let surface = surface("boot-v1");
    let look = |target| {
        preview(
            Some(&snapshot),
            Some(&surface),
            &LastSeen::default(),
            "boot-v1",
            target,
        )
    };
    assert_eq!(shown(look(NavigationTarget::Pane("w1:p1"))), Some(INNER));
    // Its tab and workspace show the whole streamed frame.
    let whole = SurfaceRect {
        x: 0,
        y: 0,
        width: 80,
        height: 24,
    };
    assert_eq!(shown(look(NavigationTarget::Tab("w1:t1"))), Some(whole));
    assert_eq!(shown(look(NavigationTarget::Workspace("w1"))), Some(whole));
}

#[test]
fn rows_off_screen_show_facts_from_the_snapshot() {
    let snapshot = snapshot();
    let surface = surface("boot-v1");
    let look = |target| {
        preview(
            Some(&snapshot),
            Some(&surface),
            &LastSeen::default(),
            "boot-v1",
            target,
        )
    };
    assert_eq!(
        facts(look(NavigationTarget::Pane("w1:p2"))),
        (
            "tail".into(),
            vec![
                ("Directory".into(), "/repo/logs".into()),
                ("Workspace".into(), "repo".into()),
                ("Tab".into(), "logs".into()),
            ]
        )
    );
    let (title, rows) = facts(look(NavigationTarget::Tab("w1:t2")));
    assert_eq!(title, "logs");
    assert_eq!(rows[0], ("Workspace".into(), "repo".into()));
    assert_eq!(rows[1], ("tail".into(), "/repo/logs".into()));
    // Without a surface, the agent's pane lists its status.
    let (_, rows) = facts(preview(
        Some(&snapshot),
        None,
        &LastSeen::default(),
        "boot-v1",
        NavigationTarget::Pane("w1:p1"),
    ));
    assert_eq!(rows[0].0, "Status");
    assert!(
        preview(
            Some(&snapshot),
            None,
            &LastSeen::default(),
            "boot-v1",
            NavigationTarget::Pane("gone")
        )
        .is_none()
    );
}

#[test]
fn nothing_from_another_daemon_boot_is_shown() {
    let snapshot = snapshot();
    // A surface from an older boot never lends its cells.
    let stale = surface("boot-v0");
    let look = preview(
        Some(&snapshot),
        Some(&stale),
        &LastSeen::default(),
        "boot-v1",
        NavigationTarget::Pane("w1:p1"),
    );
    assert!(matches!(look, Some(Preview::Facts { .. })));
    // A row listed from another boot than the host's snapshot shows nothing.
    let fresh = surface("boot-v1");
    assert!(
        preview(
            Some(&snapshot),
            Some(&fresh),
            &LastSeen::default(),
            "boot-v0",
            NavigationTarget::Pane("w1:p1")
        )
        .is_none()
    );
}

#[test]
fn spans_stay_inside_the_frame() {
    let spans = spans(INNER, 80, 24);
    assert_eq!(spans.len(), 22);
    assert_eq!((spans[0].row, spans[0].columns.clone()), (1, 1..79));
    let past = SurfaceRect {
        x: 70,
        y: 20,
        width: 20,
        height: 10,
    };
    let clipped = super::spans(past, 80, 24);
    assert_eq!(clipped.len(), 4);
    assert!(clipped.iter().all(|span| span.columns == (70..80)));
}

#[test]
fn screens_shrink_to_fit_in_half_points_but_never_grow_or_vanish() {
    let look = Look {
        font: font("Menlo"),
        size: 14.,
        line_ratio: 20. / 14.,
        cell_ratio: 0.6,
        theme: Theme::default(),
    };
    let room = |width: f32, height: f32| size(px(width), px(height));
    assert_eq!(fitted_size(&look, room(4000., 4000.), 80, 24), 14.);
    // 80 columns at 0.6 per point fit 480 pixels at exactly 10 points.
    assert_eq!(fitted_size(&look, room(480., 4000.), 80, 24), 10.);
    assert_eq!(fitted_size(&look, room(470., 4000.), 80, 24), 9.5);
    assert_eq!(fitted_size(&look, room(10., 10.), 300, 100), MIN_FONT_SIZE);
    assert_eq!(fitted_size(&look, room(480., 4000.), 0, 0), 14.);
}
