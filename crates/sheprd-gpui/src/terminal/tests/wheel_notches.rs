use super::*;

fn hit(target: InputTarget) -> WheelTarget {
    WheelTarget {
        target,
        mouse_reporting: false,
        stepwise: false,
        bounds: Bounds::default(),
        position: ClientMousePosition::Cell { column: 2, row: 3 },
        geometry: None,
    }
}

fn notch(count: f32) -> ScrollWheelEvent {
    ScrollWheelEvent {
        delta: ScrollDelta::Lines(point(0., count * f32::from(PLATFORM_NOTCH_LINES))),
        touch_phase: TouchPhase::Moved,
        ..Default::default()
    }
}

/// Smooth scrolling reads `wheel_rows` and the whole-line path reads
/// `steps`; a notch must scroll Herdr's `mouse_scroll_lines` on both.
#[test]
fn wheel_notches_scroll_the_configured_lines_and_pixels_do_not() {
    let pane = hit(InputTarget::Pane("pane".into()));
    for lines in [1, 3, 5, 7] {
        let count = NonZeroU16::new(lines).unwrap();
        let mut wheel = WheelAccumulator::default();
        let expected = lines as i16;
        assert_eq!(wheel_rows(&notch(1.), CELL_HEIGHT, count), f32::from(lines));
        assert_eq!(
            wheel
                .steps(&pane, &notch(1.), 10., CELL_HEIGHT, count)
                .lines,
            expected
        );
        assert_eq!(
            wheel
                .steps(&pane, &notch(-2.), 10., CELL_HEIGHT, count)
                .lines,
            -2 * expected
        );
    }
    // Half a notch at five lines keeps its remainder for the next half.
    let mut wheel = WheelAccumulator::default();
    let five = NonZeroU16::new(5).unwrap();
    assert_eq!(
        wheel
            .steps(&pane, &notch(0.5), 10., CELL_HEIGHT, five)
            .lines,
        2
    );
    assert_eq!(
        wheel
            .steps(&pane, &notch(0.5), 10., CELL_HEIGHT, five)
            .lines,
        3
    );
    // Large counts stay within the per-event bound.
    let huge = NonZeroU16::new(u16::MAX).unwrap();
    assert_eq!(
        wheel.steps(&pane, &notch(1.), 10., CELL_HEIGHT, huge).lines,
        128
    );
    // Trackpad pixels map one cell height to one row whatever the count.
    let pixels = ScrollWheelEvent {
        delta: ScrollDelta::Pixels(point(px(0.), px(CELL_HEIGHT * 2.))),
        touch_phase: TouchPhase::Started,
        ..Default::default()
    };
    assert_eq!(wheel_rows(&pixels, CELL_HEIGHT, five), 2.);
    let mut wheel = WheelAccumulator::default();
    assert_eq!(wheel.steps(&pane, &pixels, 10., CELL_HEIGHT, five).lines, 2);
}

/// A notch's columns are not scaled: Herdr's count is for vertical lines.
#[test]
fn horizontal_notches_ignore_mouse_scroll_lines() {
    let mut pane = hit(InputTarget::Pane("pane".into()));
    pane.mouse_reporting = true;
    let event = ScrollWheelEvent {
        delta: ScrollDelta::Lines(point(1., 0.)),
        touch_phase: TouchPhase::Moved,
        ..Default::default()
    };
    let five = NonZeroU16::new(5).unwrap();
    let mut wheel = WheelAccumulator::default();
    assert_eq!(
        wheel.steps(&pane, &event, 10., CELL_HEIGHT, five),
        WheelSteps {
            lines: 0,
            columns: 1
        }
    );
}
