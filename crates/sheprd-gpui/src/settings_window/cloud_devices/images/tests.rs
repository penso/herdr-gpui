#![allow(clippy::unwrap_used)]
use super::{SettingsWindow, ago, short};
use crate::cloud::{
    CloudProvider,
    image::{
        Progress, Project,
        builds::Running,
        catalog::{self, Entry, Image, Origin, Outcome},
    },
};
use gpui::{Entity, TestAppContext, VisualTestContext, px, size};
use std::collections::VecDeque;

fn open(cx: &mut TestAppContext) -> (Entity<SettingsWindow>, &mut VisualTestContext) {
    let main = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    let weak = cx.update(|cx| main.update(cx, |_, _, cx| cx.weak_entity()).unwrap());
    let (view, cx) = cx.add_window_view(|window, cx| {
        let view = SettingsWindow::new(weak, cx);
        window.focus(&view.focus, cx);
        view
    });
    // Tall enough that the Images card, below the container card, is in view.
    cx.simulate_resize(size(px(960.), px(2400.)));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.config.container = Default::default();
            view.select_section(super::super::super::Section::CloudDevices, window, cx);
            view.cloud_tab = CloudProvider::Container;
            cx.notify();
        })
    });
    (view, cx)
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
}

fn project(id: &str, label: &str) -> Project {
    Project {
        id: id.into(),
        label: label.into(),
    }
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let at = cx.debug_bounds(selector).unwrap().center();
    cx.simulate_click(at, Default::default());
}

#[gpui::test]
fn images_show_their_state_progress_and_log(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let images = &mut view.sandbox_images;
            images.projects = vec![Entry {
                project: project("p1", "ready-app"),
                origin: Origin {
                    endpoint_id: "local".into(),
                    path: "/src/ready-app".into(),
                },
                images: vec![Image {
                    provider: CloudProvider::Container,
                    key: "k".into(),
                    finished_at: catalog::now(),
                    outcome: Outcome::Ready {
                        reference: "herdr-ready-app-p1:abc".into(),
                    },
                    stale: Vec::new(),
                }],
            }];
            images.running = vec![Running {
                provider: CloudProvider::Container,
                project: project("p2", "building-app"),
                key: "k2".into(),
                progress: Progress {
                    step: Some((2, 4)),
                    status: "RUN make".into(),
                },
                log: VecDeque::from(["#1 [1/4] FROM base".to_owned()]),
            }];
            cx.notify();
        })
    });
    draw(cx);
    assert!(cx.debug_bounds("cloud-images").is_some());
    assert!(cx.debug_bounds("cloud-image-build-0").is_some());
    assert!(cx.debug_bounds("cloud-image-build-1").is_some());
    assert!(
        cx.debug_bounds("cloud-image-progress").is_some(),
        "the running build shows a bar"
    );
    // A ready image has no log; the running build's opens.
    assert!(cx.debug_bounds("cloud-image-log-text-1").is_none());
    click(cx, "cloud-image-log-1");
    draw(cx);
    assert!(cx.debug_bounds("cloud-image-log-text-1").is_some());
    // Reset asks once before deleting.
    click(cx, "cloud-image-reset-0");
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.sandbox_images.confirm,
            Some((CloudProvider::Container, "p1".into()))
        );
        assert!(view.sandbox_images.reset.is_none(), "nothing deleted yet");
    });
}

#[gpui::test]
fn a_provider_without_images_says_so(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    draw(cx);
    assert!(cx.debug_bounds("cloud-images").is_some());
    view.read_with(cx, |view, _| {
        assert!(view.sandbox_images.projects.is_empty())
    });
}

#[test]
fn references_and_times_read_short() {
    assert_eq!(short("herdr-app-0123:abcdef012345"), "abcdef012345");
    assert_eq!(short("herdr-app-0123-abcdef012345"), "abcdef012345");
    assert_eq!(ago(catalog::now()), "just now");
    assert_eq!(ago(catalog::now() - 120), "\u{2068}2\u{2069} min ago");
    assert_eq!(ago(catalog::now() - 7200), "\u{2068}2\u{2069} h ago");
    assert!(ago(0).ends_with("days ago"));
}
