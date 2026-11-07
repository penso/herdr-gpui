//! Devices grouped by tag: the switch over the list, which narrows it to one
//! tag, and the section heading above each group.

use super::{HOST_ARROW_WIDTH, label_text, layout::SidebarLook, line_height, scaled};
use crate::{
    SheprdWindow,
    config::corners,
    device_tags::{Daemon, DeviceTags, Tags},
    i18n::fl,
};
use gpui::{prelude::*, *};

/// The ANSI colors a tag takes, leaving out green, which says online.
const HUES: [usize; 4] = [3, 4, 5, 6];

/// A tag's color: from its letters, so it is the same on every machine.
pub(super) fn tag_color(theme: &crate::config::Theme, tag: &str) -> u32 {
    let sum: usize = tag.to_lowercase().bytes().map(usize::from).sum();
    let hue = theme.palette[HUES[sum % HUES.len()]];
    theme.ink(crate::config::mix(theme.foreground, hue, 44))
}

/// A section's key among the folds: tags match whatever their case.
fn fold_key(section: Option<&str>) -> Option<String> {
    section.map(str::to_lowercase)
}

fn in_use(tags: &Tags, tag: &str) -> bool {
    tags.all()
        .iter()
        .any(|known| sheprd_remote_control::control::tags::same(known, tag))
}

/// Whether the switch shows: with one tag, its sections already say it all.
fn switchable(tags: &[&str]) -> bool {
    tags.len() > 1
}

/// Whether `filter` can no longer be cleared, its switch or segment gone.
pub(super) fn filter_stale(tags: &Tags, filter: &str) -> bool {
    !switchable(&tags.all()) || !in_use(tags, filter)
}

impl SheprdWindow {
    /// Follows the tags as they change: a filter with no segment left to
    /// clear it would stay on.
    pub(crate) fn observe_device_tags(cx: &mut Context<Self>) -> Subscription {
        cx.observe_global::<DeviceTags>(|this, cx| {
            this.device_tags = DeviceTags::current(cx);
            let stale = this
                .device_tag_filter
                .as_deref()
                .is_some_and(|filter| filter_stale(&this.device_tags, filter));
            if stale {
                this.device_tag_filter = None;
            }
            // A tag that comes back later starts open, not hiding its devices.
            let tags = this.device_tags.clone();
            this.device_tag_folds
                .retain(|fold| fold.as_deref().is_none_or(|tag| in_use(&tags, tag)));
            cx.notify();
        })
    }

    /// Tells the tags which devices this window can reach now: this computer,
    /// and the machines connected, so no exchange waits on one that is away.
    pub(crate) fn reach_device_tags(&self, cx: &mut Context<Self>) {
        let reachable = self
            .endpoints
            .iter()
            .enumerate()
            .filter_map(|(index, endpoint)| {
                if endpoint.id == crate::endpoint::LOCAL {
                    return Some((endpoint.id.clone(), Daemon::Local));
                }
                let live = if index == self.selected_endpoint {
                    &self.live
                } else {
                    &endpoint.live
                };
                let target = &endpoint.connection.target;
                let remote = crate::automations::Machine::of(target)
                    .is_some_and(|machine| !machine.is_local());
                (endpoint.enabled && live.status.is_connected() && remote)
                    .then(|| (endpoint.id.clone(), Daemon::Remote(target.clone())))
            })
            .collect();
        DeviceTags::reach(cx, reachable);
    }

    /// Lists only `tag`'s devices, from the top, as a device scope does.
    fn set_device_tag_filter(&mut self, tag: Option<String>, cx: &mut Context<Self>) {
        self.device_tag_filter = tag;
        for scroll in &self.sidebar_scroll {
            scroll.set_offset(Point::default());
        }
        for revealed in &self.sidebar_revealed {
            revealed.set(None);
        }
        self.sidebar_pin_reveal.set(None);
        cx.notify();
    }

    /// The switch over the device list, while there are tags to choose from.
    pub(super) fn tag_switch(&self, cx: &mut Context<Self>) -> Option<Div> {
        let tags = self.device_tags.all();
        if !switchable(&tags) {
            return None;
        }
        let segments = std::iter::once(None)
            .chain(tags.into_iter().map(Some))
            .map(|tag| self.tag_segment(tag, cx));
        Some(
            div()
                .debug_selector(|| "device-tag-switch".into())
                .flex()
                .flex_none()
                .flex_wrap()
                .mx_2()
                .mt_2()
                .mb_2()
                .p(px(2.))
                .rounded(px(corners::CONTROL))
                .bg(rgb(self.theme.active))
                .children(segments),
        )
    }

    fn tag_segment(&self, tag: Option<&str>, cx: &mut Context<Self>) -> Stateful<Div> {
        let font = &self.config.sidebar;
        let active = self.device_tag_filter.as_deref() == tag;
        let label: SharedString = tag
            .map_or_else(|| fl!("device-tags-all"), str::to_owned)
            .into();
        let chosen = tag.map(str::to_owned);
        let name = format!("device-tag-segment-{}", tag.unwrap_or_default());
        div()
            .id(SharedString::from(name.clone()))
            .debug_selector(move || name.clone())
            .role(Role::Button)
            .aria_label(label.clone())
            .flex()
            .flex_grow(1.)
            .justify_center()
            .min_w_0()
            .px(px(6.))
            .py(px(2.))
            .rounded(px(corners::SMALL))
            .text_size(px(font.size - 1.))
            .truncate()
            .when(active, |segment| {
                segment
                    .bg(rgb(self.theme.surface))
                    .font_weight(FontWeight::SEMIBOLD)
            })
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, _, cx| {
                this.set_device_tag_filter(chosen.clone(), cx);
            }))
            .child(label)
    }

    /// Whether `id` is in a section folded away. Without tags there are no
    /// sections, so a fold left on Untagged hides nothing.
    pub(super) fn device_tag_folded(&self, id: &str) -> bool {
        self.device_tags.any()
            && self.device_tag_folds.contains(&fold_key(
                self.device_tags
                    .section(id, self.device_tag_filter.as_deref()),
            ))
    }

    /// Folds or unfolds a section, as a host's arrow does its workspaces.
    fn toggle_device_tag_fold(&mut self, key: Option<String>, cx: &mut Context<Self>) {
        if !self.device_tag_folds.remove(&key) {
            self.device_tag_folds.insert(key);
        }
        cx.notify();
    }

    /// The heading above a section of `count` devices, which folds it.
    /// `None` is Untagged.
    pub(super) fn tag_section(
        &self,
        tag: Option<&str>,
        count: usize,
        look: SidebarLook,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let font = &self.config.sidebar;
        let theme = &self.theme;
        let color = tag.map_or(theme.muted, |tag| tag_color(theme, tag));
        let label = tag.map_or_else(|| fl!("device-tags-untagged"), str::to_owned);
        let name = format!("device-tag-section-{}", tag.unwrap_or_default());
        let dot = scaled(font, 7.);
        let arrow = format!("device-tag-arrow-{}", tag.unwrap_or_default());
        let key = fold_key(tag);
        let folded = self.device_tag_folds.contains(&key);
        let spoken = if folded {
            fl!("device-tags-folded", group = label.as_str())
        } else {
            label.clone()
        };
        div()
            .id(SharedString::from(name.clone()))
            .debug_selector(move || name.clone())
            .role(Role::Button)
            .aria_label(SharedString::from(spoken))
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, _, cx| {
                this.toggle_device_tag_fold(key.clone(), cx);
            }))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(look.density.host_gap()))
            .px(px(look.content_x()))
            .pt_1()
            .h(px(line_height(font) + 4.))
            .text_size(px(font.size - 2.))
            // The host's arrow, in its place and at its size, over the
            // heading's smaller text.
            .child(
                div()
                    .debug_selector(move || arrow.clone())
                    .flex_none()
                    .w(px(scaled(font, HOST_ARROW_WIDTH)))
                    .text_size(px(font.size))
                    .text_color(rgb(theme.foreground))
                    .child(label_text(if folded {
                        crate::direction::glyph("\u{25b8}")
                    } else {
                        "\u{25be}"
                    })),
            )
            .child(
                div()
                    .flex_none()
                    .size(px(dot))
                    .rounded_full()
                    .bg(rgb(color)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(color))
                    .child(label.to_uppercase()),
            )
            .child(
                div()
                    .flex_none()
                    .text_color(rgb(theme.subtext()))
                    .child(count.to_string()), // i18n: data
            )
    }

    /// How many listed devices share `section`.
    pub(super) fn tag_section_count(&self, section: Option<&str>) -> usize {
        let filter = self.device_tag_filter.as_deref();
        self.endpoints
            .iter()
            .filter(|endpoint| self.device_visible(&endpoint.id))
            .filter(|endpoint| {
                let own = self.device_tags.section(&endpoint.id, filter);
                match (own, section) {
                    (Some(own), Some(section)) => {
                        sheprd_remote_control::control::tags::same(own, section)
                    }
                    (None, None) => true,
                    _ => false,
                }
            })
            .count()
    }
}
