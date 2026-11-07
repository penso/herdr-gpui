use super::*;

/// Go To's preview sits beside its list when the window has room, Alt-P
/// hides it, and a narrow window keeps the list alone.
#[gpui::test]
fn go_to_preview_sits_beside_the_list_when_there_is_room(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        fixture_window(window, cx)
    });
    let open = |cx: &mut VisualTestContext, filter| {
        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.open_palette(filter, window, cx));
            window.draw(cx).clear(cx);
        });
        // The list lays its rows out on the frame after it is sized.
        cx.update(|window, cx| window.draw(cx).clear(cx));
    };
    cx.simulate_resize(size(px(1400.), px(900.)));
    open(cx, Filter::Navigation);
    let row = cx.debug_bounds("palette-row-0").unwrap();
    let preview = cx.debug_bounds("palette-preview").unwrap();
    assert!(preview.left() >= row.right());
    assert!(preview.right() <= px(1400.));
    assert!(preview.size.height > px(100.));

    cx.simulate_keystrokes("alt-p");
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("palette-preview").is_none());
    cx.simulate_keystrokes("alt-p");
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("palette-preview").is_some());

    // Only Go To previews.
    cx.simulate_keystrokes("escape");
    open(cx, Filter::Commands);
    assert!(cx.debug_bounds("palette-preview").is_none());

    cx.simulate_keystrokes("escape");
    cx.simulate_resize(size(px(700.), px(700.)));
    open(cx, Filter::Navigation);
    assert!(cx.debug_bounds("palette-preview").is_none());
    assert!(cx.debug_bounds("palette-row-0").unwrap().right() <= px(700.));
}

/// The selected pane's cells are painted when its host streams its tab.
#[gpui::test]
fn a_streamed_pane_previews_its_cells(cx: &mut TestAppContext) {
    use sheprd_client::protocol::{
        CellData, FrameData, PaneSurfaceFrame, PaneSurfacePane, SurfaceRect,
    };
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        let snapshot = view.live.snapshot.clone().unwrap();
        let cell = CellData {
            symbol: "x".into(),
            fg: 0,
            bg: 0,
            modifier: 0,
            skip: false,
            hyperlink: None,
        };
        let whole = SurfaceRect {
            x: 0,
            y: 0,
            width: 80,
            height: 24,
        };
        view.live.surface = Some(Arc::new(PaneSurfaceFrame {
            boot_id: snapshot.boot_id.clone(),
            projection_revision: snapshot.revision,
            surface_revision: 1,
            frame: FrameData {
                width: 80,
                height: 24,
                cells: vec![cell; 80 * 24],
                cursor: None,
                hyperlinks: vec![],
                graphics: vec![],
            },
            panes: vec![PaneSurfacePane {
                pane_id: snapshot.panes[0].pane_id.clone(),
                content_revision: 1,
                rect: whole,
                inner_rect: whole,
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
        }));
        view
    });
    cx.simulate_resize(size(px(1400.), px(900.)));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_palette(Filter::Navigation, window, cx);
            let palette = view.menu.palette.as_mut().unwrap();
            palette.selected = palette
                .filtered
                .iter()
                .position(|hit| {
                    matches!(
                        palette.entries[hit.index].action,
                        Action::Go {
                            target: NavigationTarget::Pane(_),
                            ..
                        }
                    )
                })
                .unwrap();
        });
        window.draw(cx).clear(cx);
        window.draw(cx).clear(cx);
    });
    view.read_with(cx, |view, _| {
        let palette = view.menu.palette.as_ref().unwrap();
        assert!(matches!(
            view.palette_preview(palette),
            Some(preview::Preview::Screen { .. })
        ));
    });
    assert!(cx.debug_bounds("palette-preview").is_some());
    assert!(cx.debug_bounds("palette-preview-age").is_none());

    // Once its host streams another tab, the pane shows its last frame, aged.
    cx.update(|window, cx| {
        view.update(cx, |view, _| {
            let surface = view.live.surface.take();
            let snapshot = view.live.snapshot.clone();
            view.endpoints[view.selected_endpoint].last_seen.observe(
                (snapshot.as_deref(), surface.as_ref()),
                (snapshot.as_deref(), None),
                unix_now(),
            );
        });
        window.draw(cx).clear(cx);
    });
    view.read_with(cx, |view, _| {
        let palette = view.menu.palette.as_ref().unwrap();
        assert!(matches!(
            view.palette_preview(palette),
            Some(preview::Preview::Screen {
                seen_at: Some(_),
                ..
            })
        ));
    });
    assert!(cx.debug_bounds("palette-preview-age").is_some());
}
