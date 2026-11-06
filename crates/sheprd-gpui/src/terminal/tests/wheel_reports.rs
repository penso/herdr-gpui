use super::*;

fn surface(mouse_reporting: bool, alternate_screen_active: bool) -> PaneSurfaceFrame {
    let frame = FrameData {
        cells: vec![],
        width: 80,
        height: 24,
        cursor: None,
        hyperlinks: vec![],
        graphics: vec![],
    };
    PaneSurfaceFrame {
        boot_id: "boot".into(),
        projection_revision: 1,
        surface_revision: 1,
        frame,
        splits: vec![],
        popup: None,
        graphics: Default::default(),
        panes: vec![PaneSurfacePane {
            pane_id: "pane".into(),
            content_revision: 1,
            rect: SurfaceRect {
                x: 0,
                y: 0,
                width: 40,
                height: 24,
            },
            inner_rect: SurfaceRect {
                x: 1,
                y: 1,
                width: 38,
                height: 22,
            },
            scrollbar_rect: None,
            scroll: None,
            focused: false,
            mouse_reporting,
            sgr_pixel_mouse: false,
            alternate_screen_active,
            pixel_width: 380,
            pixel_height: 440,
        }],
    }
}

fn counts(surface: &PaneSurfaceFrame, lines: i16) -> Vec<u16> {
    wheel_target(surface, 35., 65., 10., CELL_HEIGHT)
        .unwrap()
        .wheel_events(WheelSteps { lines, columns: 0 }, Modifiers::default())
        .map(|event| match event {
            ClientPaneInputEvent::Mouse { lines, .. } => lines,
            event => panic!("unexpected wheel event {event:?}"),
        })
        .collect()
}

/// Herdr sends an application reading the wheel one report, or one arrow key
/// on the alternate screen, per event whatever its count, so a fast swipe of
/// five rows has to reach Claude Code or Codex as five events, as it does
/// from a terminal. The scrollback still moves by the whole count at once.
#[test]
fn an_application_reading_the_wheel_gets_every_step() {
    assert_eq!(counts(&surface(false, false), 5), [5]);
    assert_eq!(counts(&surface(true, false), 5), [1; 5]);
    assert_eq!(counts(&surface(false, true), -5), [1; 5]);
    assert_eq!(counts(&surface(true, true), 128), [1; 128]);
}
