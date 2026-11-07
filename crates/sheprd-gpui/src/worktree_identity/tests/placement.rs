//! Where the rule lies in a tab group, drawn headlessly.
use super::*;
use gpui::{
    Context, InteractiveElement, IntoElement, ParentElement, Render, TestAppContext, Window,
    prelude::FluentBuilder, size,
};

const STRIP: f32 = 30.;

/// A group as `render_group` lays it out: a strip above or below a body, or
/// a body alone, with the rule placed as the group places it.
struct Group {
    strip: Option<bool>,
}

impl Render for Group {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let below = self.strip == Some(true);
        let strip = self.strip.map(|_| {
            div()
                .debug_selector(|| "strip".into())
                .relative()
                .flex_none()
                .h(px(STRIP))
        });
        let rule = Worktrees::new(None, &[Surface::Terminal])
            .rule(true, &Theme::default())
            .map(|rule| rule.debug_selector(|| "rule".into()));
        let strip = place_rule(strip, rule, below);
        let body = div().debug_selector(|| "body".into()).flex_1().min_h_0();
        div()
            .size_full()
            .flex()
            .flex_col()
            .map(|column| match (strip, below) {
                (Some(strip), false) => column.child(strip).child(body),
                (Some(strip), true) => column.child(body).child(strip),
                (None, _) => column.child(body),
            })
    }
}

/// The rule lies inside the strip, on the edge that meets the terminals,
/// so it covers no cell and the body keeps its full size; with no strip
/// there is no chrome to hold it, and it is not drawn over the cells.
#[gpui::test]
fn the_rule_lies_on_the_strips_inner_edge_and_never_covers_the_body(cx: &mut TestAppContext) {
    for strip in [Some(false), Some(true), None] {
        let (_, cx) = cx.add_window_view(|_, _| Group { strip });
        cx.simulate_resize(size(px(640.), px(400.)));
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.refresh();
            let _ = window.draw(cx);
        });
        let rule = cx.debug_bounds("rule");
        let body = cx.debug_bounds("body").unwrap();
        match strip {
            Some(below) => {
                let rule = rule.unwrap();
                assert_eq!(rule.size, size(px(640.), px(RULE)), "{below}");
                let strip = cx.debug_bounds("strip").unwrap();
                assert_eq!(body.size.height, px(400. - STRIP), "{below}");
                if below {
                    assert_eq!(rule.top(), strip.top());
                    assert_eq!(rule.top(), body.bottom());
                } else {
                    assert_eq!(rule.bottom(), strip.bottom());
                    assert_eq!(rule.bottom(), body.top());
                }
            }
            None => {
                assert_eq!(body.size.height, px(400.));
                assert!(rule.is_none());
            }
        }
    }
}
