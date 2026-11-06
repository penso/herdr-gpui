use super::*;

#[core::prelude::v1::test]
fn manual_rows_are_bounded_without_losing_the_choice_after_a_resize() {
    let chosen = InputSize::Rows(17);
    for (height, expected) in [(40, 17), (12, 9), (5, 3), (2, 2), (0, 0), (40, 17)] {
        let rect = SurfaceRect { height, ..RECT };
        let actual = layout(rect, chosen, true, None, VIEWPORT);
        assert_eq!(actual.input.height, expected);
        assert_eq!(actual.input.y + actual.input.height, rect.y + height);
        assert_eq!(
            actual.conversation.height + actual.divider.height + actual.input.height,
            height
        );
    }
    let small = layout(RECT, InputSize::Rows(0), false, None, VIEWPORT);
    assert_eq!(small.input.height, 3);
    let large = layout(RECT, InputSize::Rows(u16::MAX), false, None, VIEWPORT);
    assert_eq!(large.input.height, RECT.height - 3);
    assert_eq!(large.conversation.height, 2);
}

#[core::prelude::v1::test]
fn manual_height_still_exposes_the_live_editor_cursor() {
    let cursor = CursorState {
        x: RECT.x,
        y: RECT.y + 8,
        visible: true,
        shape: Default::default(),
    };
    let actual = layout(RECT, InputSize::Rows(3), false, Some(&cursor), VIEWPORT);
    assert_eq!(actual.input.y, cursor.y - 1);
    assert_eq!(actual.input.y + actual.input.height, RECT.y + RECT.height);
    // A blocked agent's question opens the automatic size only.
    let blocked = layout(RECT, InputSize::Auto, true, None, VIEWPORT);
    assert_eq!(blocked.state, InputState::Blocked);
    assert_eq!(
        Layout {
            state: InputState::Ready,
            ..blocked
        },
        layout(RECT, InputSize::Expanded, false, None, VIEWPORT)
    );
    assert_eq!(
        layout(RECT, InputSize::Auto, false, None, VIEWPORT),
        Layout {
            state: InputState::Ready,
            ..layout(RECT, InputSize::Compact, true, None, VIEWPORT)
        }
    );
}
