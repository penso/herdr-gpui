//! Devices grouped by tag: sections in the list, and the switch above it.

use super::*;
use crate::device_tags::Tags;
use std::sync::Arc;

fn remote(id: &str, view: &SheprdWindow) -> crate::endpoint::Endpoint {
    let mut endpoint = crate::endpoint::Endpoint::new(
        id.into(),
        id.into(),
        ConnectTarget::Ssh {
            target: "example.invalid".into(),
            session: "default".into(),
        },
        true,
    );
    endpoint.live.snapshot = view.live.snapshot.clone();
    endpoint
}

fn grouped_window<'a>(
    cx: &'a mut gpui::TestAppContext,
    tags: &[(&str, &[&str])],
) -> (Entity<SheprdWindow>, &'a mut gpui::VisualTestContext) {
    let tags = Arc::new(Tags::tagged(tags));
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        let view = cx.new(|cx| fixture_window(window, cx));
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        SidebarFixture(view)
    });
    let view = cx.update(|_, cx| fixture.read(cx).0.clone());
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            for id in ["ssh:a", "ssh:b", "ssh:c"] {
                let endpoint = remote(id, view);
                view.endpoints.push(endpoint);
            }
            view.device_tags = tags;
        })
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.run_until_parked();
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    (view, cx)
}

fn listed(view: &Entity<SheprdWindow>, cx: &mut gpui::VisualTestContext) -> Vec<String> {
    view.read_with(cx, |view, _| {
        view.displayed_devices()
            .into_iter()
            .filter(|(_, endpoint)| view.device_visible(&endpoint.id))
            .map(|(_, endpoint)| endpoint.id.clone())
            .collect()
    })
}

#[gpui::test]
fn without_tags_the_list_is_as_it_was(cx: &mut gpui::TestAppContext) {
    let (view, cx) = grouped_window(cx, &[]);
    assert!(cx.debug_bounds("device-tag-switch").is_none());
    assert!(cx.debug_bounds("device-tag-section-").is_none());
    assert_eq!(listed(&view, cx), ["local", "ssh:a", "ssh:b", "ssh:c"]);
}

#[gpui::test]
fn devices_list_in_sections_with_untagged_last(cx: &mut gpui::TestAppContext) {
    let (view, cx) = grouped_window(
        cx,
        &[("ssh:a", &["work"]), ("ssh:c", &["personal", "work"])],
    );
    assert_eq!(listed(&view, cx), ["ssh:c", "ssh:a", "local", "ssh:b"]);
    let top =
        |cx: &mut gpui::VisualTestContext, selector| cx.debug_bounds(selector).unwrap().origin.y;
    let personal = top(cx, "device-tag-section-personal");
    let work = top(cx, "device-tag-section-work");
    let untagged = top(cx, "device-tag-section-");
    assert!(personal < work && work < untagged);
    // The list scrolled to the device on screen; the switch stays above it.
    assert!(top(cx, "device-tag-switch") < untagged);
}

#[gpui::test]
fn the_switch_narrows_the_list_and_keeps_the_device_on_screen(cx: &mut gpui::TestAppContext) {
    let (view, cx) = grouped_window(
        cx,
        &[("ssh:a", &["work"]), ("ssh:c", &["personal", "work"])],
    );
    let segment = cx.debug_bounds("device-tag-segment-work").unwrap().center();
    cx.simulate_click(segment, Modifiers::default());
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    assert_eq!(
        view.read_with(cx, |view, _| view.device_tag_filter.clone()),
        Some("work".to_owned())
    );
    // Local is on screen, so it stays, under its own section.
    assert_eq!(listed(&view, cx), ["ssh:a", "ssh:c", "local"]);
    assert!(cx.debug_bounds("device-tag-section-personal").is_none());

    let all = cx.debug_bounds("device-tag-segment-").unwrap().center();
    cx.simulate_click(all, Modifiers::default());
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    assert_eq!(
        view.read_with(cx, |view, _| view.device_tag_filter.clone()),
        None
    );
    assert_eq!(listed(&view, cx).len(), 4);
}

#[gpui::test]
fn stepping_through_workspaces_follows_the_sections(cx: &mut gpui::TestAppContext) {
    let (view, cx) = grouped_window(cx, &[("ssh:c", &["personal"]), ("ssh:a", &["work"])]);
    let hosts = view.read_with(cx, |view, _| {
        let mut hosts: Vec<usize> = view
            .sidebar_workspaces()
            .into_iter()
            .map(|(host, _)| host)
            .collect();
        hosts.dedup();
        hosts
            .into_iter()
            .map(|index| view.endpoints[index].id.clone())
            .collect::<Vec<_>>()
    });
    assert_eq!(hosts, ["ssh:c", "ssh:a", "local", "ssh:b"]);
}

#[gpui::test]
fn a_section_folds_and_unfolds_from_its_heading(cx: &mut gpui::TestAppContext) {
    let (view, cx) = grouped_window(cx, &[("ssh:a", &["work"]), ("ssh:c", &["Work"])]);
    // The list scrolled to the device on screen, past this heading.
    view.update(cx, |view, _| {
        view.sidebar_scroll[0].set_offset(gpui::Point::default())
    });
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    let toggle = |cx: &mut gpui::VisualTestContext| {
        let heading = cx.debug_bounds("device-tag-section-work").unwrap().center();
        cx.simulate_click(heading, Modifiers::default());
        cx.update(|window, cx| full_draw(window, cx).clear(cx));
    };
    assert!(cx.debug_bounds("host-label-ssh:a").is_some());

    toggle(cx);
    // The heading stays to unfold it; the other section is untouched.
    assert!(cx.debug_bounds("device-tag-section-work").is_some());
    assert!(cx.debug_bounds("host-label-ssh:a").is_none());
    assert!(cx.debug_bounds("host-label-ssh:c").is_none());
    assert!(cx.debug_bounds("host-label-ssh:b").is_some());
    let hosts = view.read_with(cx, |view, _| {
        view.sidebar_workspaces()
            .iter()
            .map(|(host, _)| view.endpoints[*host].id.clone())
            .collect::<Vec<_>>()
    });
    assert!(!hosts.iter().any(|id| id == "ssh:a" || id == "ssh:c"));

    toggle(cx);
    assert!(cx.debug_bounds("host-label-ssh:a").is_some());
    assert!(cx.debug_bounds("host-label-ssh:c").is_some());
}

#[gpui::test]
fn a_fold_on_untagged_hides_nothing_once_no_tags_remain(cx: &mut gpui::TestAppContext) {
    let (view, cx) = grouped_window(cx, &[("ssh:a", &["work"])]);
    view.update(cx, |view, _| {
        view.device_tag_folds.insert(None);
        view.device_tag_folds.insert(Some("work".to_owned()));
        assert!(view.device_tag_folded("ssh:a") && view.device_tag_folded("local"));
        view.device_tags = Arc::new(Tags::tagged(&[]));
        assert!(!view.device_tag_folded("local"));
    });
}

#[gpui::test]
fn devices_step_in_under_their_section(cx: &mut gpui::TestAppContext) {
    let (view, cx) = grouped_window(cx, &[("ssh:a", &["work"])]);
    // A little past the first host's header, so its copy is pinned.
    let row = cx.debug_bounds("host-ssh:a").unwrap();
    let list = view.read_with(cx, |view, _| view.sidebar_scroll[0].clone());
    let offset = list.offset().y + cx.debug_bounds("header-spaces").unwrap().bottom() - row.top();
    list.set_offset(point(px(0.), offset - px(30.)));
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    let host = cx.debug_bounds("sticky-collapse-host-ssh:a").unwrap();
    let section = cx.debug_bounds("device-tag-arrow-work").unwrap();
    assert_eq!(section.size.width, host.size.width);
    assert!(host.origin.x > section.right());

    // Without sections the host is back beside the edge, where the heading's
    // arrow was.
    let label = cx.debug_bounds("host-label-ssh:a").unwrap();
    view.update(cx, |view, _| view.device_tags = Arc::new(Tags::tagged(&[])));
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    let plain = cx.debug_bounds("host-label-ssh:a").unwrap();
    assert_eq!(
        label.origin.x - plain.origin.x,
        host.origin.x - section.origin.x
    );
}

#[gpui::test]
fn the_switch_keeps_a_gap_below_the_heading_above_it(cx: &mut gpui::TestAppContext) {
    let (_, cx) = grouped_window(cx, &[("ssh:a", &["work"]), ("ssh:b", &["personal"])]);
    let heading = cx.debug_bounds("header-spaces").unwrap();
    let switch = cx.debug_bounds("device-tag-switch").unwrap();
    assert!(switch.origin.y >= heading.bottom() + px(8.));
}

#[gpui::test]
fn one_tag_has_sections_but_no_switch(cx: &mut gpui::TestAppContext) {
    let (_, cx) = grouped_window(cx, &[("ssh:a", &["work"]), ("ssh:b", &["Work"])]);
    assert!(cx.debug_bounds("device-tag-switch").is_none());
    assert!(cx.debug_bounds("device-tag-section-work").is_some());
}

#[test]
fn a_filter_goes_with_its_switch_or_its_segment() {
    use crate::sidebar::tag_groups::filter_stale;
    let two = Tags::tagged(&[("ssh:a", &["work"]), ("ssh:b", &["personal"])]);
    assert!(!filter_stale(&two, "Work"));
    assert!(filter_stale(&two, "lab"));
    assert!(filter_stale(&Tags::tagged(&[("ssh:a", &["work"])]), "work"));
}

#[gpui::test]
fn two_spellings_of_a_tag_are_one_section(cx: &mut gpui::TestAppContext) {
    let (_, cx) = grouped_window(cx, &[("ssh:a", &["work"]), ("ssh:b", &["Work"])]);
    assert!(cx.debug_bounds("device-tag-section-work").is_some());
    assert!(cx.debug_bounds("device-tag-section-Work").is_none());
}
