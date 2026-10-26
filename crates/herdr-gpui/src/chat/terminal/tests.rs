use super::*;

const VIEWPORT: ClientSurfaceSize = ClientSurfaceSize {
    cols: 200,
    rows: 100,
};

const RECT: SurfaceRect = SurfaceRect {
    x: 4,
    y: 3,
    width: 80,
    height: 40,
};

#[core::prelude::v1::test]
fn compact_input_keeps_eight_original_rows_and_a_divider() {
    let actual = layout(RECT, InputSize::Compact, false, None, VIEWPORT);
    assert_eq!(actual.conversation.height, 31);
    assert_eq!(actual.divider.y, 34);
    assert_eq!(
        actual.input,
        SurfaceRect {
            y: 35,
            height: 8,
            ..RECT
        }
    );
    assert_eq!(
        actual.conversation.height + actual.divider.height + actual.input.height,
        RECT.height
    );
}

#[core::prelude::v1::test]
fn expanded_input_exposes_dialogs_without_moving_terminal_coordinates() {
    let actual = layout(RECT, InputSize::Expanded, false, None, VIEWPORT);
    assert_eq!(actual.conversation.height, 2);
    assert_eq!(
        actual.input,
        SurfaceRect {
            y: 6,
            height: 37,
            ..RECT
        }
    );
}

#[core::prelude::v1::test]
fn a_multiline_editor_grows_upward_to_keep_its_cursor_visible() {
    let cursor = CursorState {
        x: 6,
        y: 20,
        visible: true,
        shape: Default::default(),
    };
    let actual = layout(RECT, InputSize::Compact, false, Some(&cursor), VIEWPORT);
    assert_eq!(actual.input.y, 19);
    assert_eq!(actual.input.y + actual.input.height, RECT.y + RECT.height);
    let elsewhere = CursorState {
        x: 0,
        ..cursor.clone()
    };
    assert_eq!(
        layout(RECT, InputSize::Compact, false, Some(&elsewhere), VIEWPORT),
        layout(RECT, InputSize::Compact, false, None, VIEWPORT)
    );
    let hidden = CursorState {
        visible: false,
        ..cursor
    };
    assert_eq!(
        layout(RECT, InputSize::Compact, false, Some(&hidden), VIEWPORT),
        layout(RECT, InputSize::Compact, false, None, VIEWPORT)
    );
}

#[core::prelude::v1::test]
fn tiny_panes_keep_their_complete_original_editor() {
    for height in 0..9 {
        let rect = SurfaceRect { height, ..RECT };
        for expanded in [false, true] {
            let actual = layout(rect, InputSize::Auto, expanded, None, VIEWPORT);
            assert_eq!(actual.input, rect);
            assert_eq!(actual.conversation.height, 0);
            assert_eq!(actual.divider.height, 0);
        }
    }
}

#[core::prelude::v1::test]
fn a_larger_daemon_frame_shows_an_affordance_inside_the_actual_viewport() {
    let viewport = ClientSurfaceSize { cols: 60, rows: 30 };
    for expanded in [false, true] {
        let actual = layout(RECT, InputSize::Auto, expanded, None, viewport);
        assert_eq!(actual.state, InputState::AwaitingFit);
        assert_eq!(
            actual.conversation,
            SurfaceRect {
                width: 56,
                height: 26,
                ..RECT
            }
        );
        assert_eq!(
            actual.divider,
            SurfaceRect {
                y: 29,
                height: 1,
                width: 56,
                ..RECT
            }
        );
        assert_eq!(
            actual.input.height, 0,
            "no clipped terminal tail is presented as input"
        );
        assert_eq!(actual.input.y, viewport.rows);
    }
    let fitted = layout(RECT, InputSize::Compact, false, None, VIEWPORT);
    assert_eq!(fitted.state, InputState::Ready);
    assert_eq!(fitted.input.height, 8);
}

mod resize;
