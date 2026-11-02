//! Live sidebar spacing, selection and host colours: previews reach every main
//! window at once, and commits write through Settings' serial save path.
// i18n: migrated
use super::{SettingsWindow, ui};
use crate::{
    config::{Config, SidebarStyle, preferences::Preference},
    search_input::SearchInput,
};
use gpui::{prelude::*, *};
use std::collections::BTreeSet;

/// One `[sidebar]` key a draft can change.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum SidebarKey {
    Indent,
    RowPadding,
    Gap,
    HostGap,
    Select,
    DeviceFooter,
    ActivityBar,
    Automations,
    ChangeCounts,
    Host(String),
}

impl SidebarKey {
    const FIXED: &[Self] = &[
        Self::Indent,
        Self::RowPadding,
        Self::Gap,
        Self::HostGap,
        Self::Select,
        Self::DeviceFooter,
        Self::ActivityBar,
        Self::Automations,
        Self::ChangeCounts,
    ];

    /// Copies this key's value from `from` into `to`.
    fn copy(&self, from: &SidebarStyle, to: &mut SidebarStyle) {
        let (old, new) = (&from.overrides, &mut to.overrides);
        match self {
            Self::Indent => new.indent = old.indent,
            Self::RowPadding => new.row_padding = old.row_padding,
            Self::Gap => new.gap = old.gap,
            Self::HostGap => new.host_gap = old.host_gap,
            Self::Select => to.select = from.select,
            Self::DeviceFooter => to.device_footer = from.device_footer,
            Self::ActivityBar => to.activity_bar = from.activity_bar,
            Self::Automations => to.automations = from.automations,
            Self::ChangeCounts => to.change_counts = from.change_counts,
            Self::Host(host) => match from.hosts.get(host) {
                Some(color) => {
                    to.hosts.insert(host.clone(), *color);
                }
                None => {
                    to.hosts.remove(host);
                }
            },
        }
    }

    /// The write that puts `style`'s value for this key in the file.
    fn preference(&self, style: &SidebarStyle) -> Preference {
        let overrides = &style.overrides;
        match self {
            Self::Indent => Preference::SidebarIndent(overrides.indent),
            Self::RowPadding => Preference::SidebarRowPadding(overrides.row_padding),
            Self::Gap => Preference::SidebarRowGap(overrides.gap),
            Self::HostGap => Preference::SidebarHostGap(overrides.host_gap),
            Self::Select => Preference::SidebarSelect(style.select),
            Self::DeviceFooter => Preference::SidebarDeviceFooter(style.device_footer),
            Self::ActivityBar => Preference::SidebarActivityBar(style.activity_bar),
            Self::Automations => Preference::SidebarAutomations(style.automations),
            Self::ChangeCounts => Preference::SidebarChangeCounts(style.change_counts),
            Self::Host(host) => Preference::HostColor {
                host: host.clone(),
                color: style.hosts.get(host).copied(),
            },
        }
    }

    fn differs(&self, before: &SidebarStyle, after: &SidebarStyle) -> bool {
        let (old, new) = (&before.overrides, &after.overrides);
        match self {
            Self::Indent => old.indent != new.indent,
            Self::RowPadding => old.row_padding != new.row_padding,
            Self::Gap => old.gap != new.gap,
            Self::HostGap => old.host_gap != new.host_gap,
            Self::Select => before.select != after.select,
            Self::DeviceFooter => before.device_footer != after.device_footer,
            Self::ActivityBar => before.activity_bar != after.activity_bar,
            Self::Automations => before.automations != after.automations,
            Self::ChangeCounts => before.change_counts != after.change_counts,
            Self::Host(host) => before.hosts.get(host) != after.hosts.get(host),
        }
    }
}

/// The keys a draft has changed since it started. Only these are written,
/// and a reload keeps every other key as the file has it.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct SidebarTouched(BTreeSet<SidebarKey>);

impl SidebarTouched {
    /// Every key whose value differs between `before` and `after`.
    fn between(before: &SidebarStyle, after: &SidebarStyle) -> Self {
        let hosts = before
            .hosts
            .keys()
            .chain(after.hosts.keys())
            .map(|host| SidebarKey::Host(host.clone()));
        Self(
            SidebarKey::FIXED
                .iter()
                .cloned()
                .chain(hosts)
                .filter(|key| key.differs(before, after))
                .collect(),
        )
    }

    fn extend(&mut self, other: &Self) {
        self.0.extend(other.0.iter().cloned());
    }

    fn keys(&self) -> impl Iterator<Item = &SidebarKey> {
        self.0.iter()
    }
}

/// What the Sidebar page shows and every main window uses until it is saved.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct SidebarIntent {
    pub style: SidebarStyle,
    /// Filled in by `preview_sidebar` from what the draft changed.
    pub touched: SidebarTouched,
}

impl SidebarIntent {
    pub(super) fn of(config: &Config) -> Self {
        Self {
            style: config.sidebar_style.clone(),
            touched: SidebarTouched::default(),
        }
    }

    /// Every key, touched or not.
    pub(super) fn apply(&self, config: &mut Config) {
        config.sidebar_style = self.style.clone();
    }

    /// Only the keys this draft changed, over whatever `config` holds.
    fn overlay(&self, config: &mut Config) {
        for key in self.touched.keys() {
            key.copy(&self.style, &mut config.sidebar_style);
        }
    }

    /// Takes every untouched key from a fresh load, so a later write and
    /// the windows see what the file says there.
    fn rebase(&mut self, loaded: &Config) {
        let mut style = loaded.sidebar_style.clone();
        for key in self.touched.keys() {
            key.copy(&self.style, &mut style);
        }
        self.style = style;
    }
}

/// The host colour being typed as hex, for the host it was opened on: rows
/// can reorder while it is open.
pub(super) struct HexEdit {
    pub host: String,
    pub input: Entity<SearchInput>,
    pub invalid: bool,
    pub _blur: Subscription,
}

#[derive(Default)]
struct SidebarDraft {
    active: bool,
    revision: u64,
    intent: Option<SidebarIntent>,
}
impl Global for SidebarDraft {}

pub(crate) fn sidebar_load_revision(cx: &App) -> u64 {
    cx.try_global::<SidebarDraft>()
        .map_or(0, |draft| draft.revision)
}

pub(crate) fn apply_loaded_sidebar(config: &mut Config, revision: u64, cx: &App) {
    if let Some(draft) = cx.try_global::<SidebarDraft>()
        && (draft.active || draft.revision != revision)
        && let Some(intent) = &draft.intent
    {
        // Reads started before a commit must not put the old spacing back,
        // nor the draft's copy of a key it never changed over the file's.
        intent.overlay(config);
    }
}

fn clear_sidebar_draft(cx: &mut App) {
    let draft = cx.default_global::<SidebarDraft>();
    draft.active = false;
    draft.revision = draft.revision.wrapping_add(1);
}

/// Every touched key whose value differs from `saved`, in one save's order.
fn diff_preferences(saved: &Config, intent: &SidebarIntent) -> Vec<Preference> {
    intent
        .touched
        .keys()
        .filter(|key| key.differs(&saved.sidebar_style, &intent.style))
        .map(|key| key.preference(&intent.style))
        .collect()
}

/// Every touched key of `intent`, for a quit that cannot know what an
/// in-flight save left on disk: a revert to the loaded value must still land.
fn touched_preferences(intent: &SidebarIntent) -> Vec<Preference> {
    intent
        .touched
        .keys()
        .map(|key| key.preference(&intent.style))
        .collect()
}

impl SettingsWindow {
    pub(super) fn sidebar_dirty(&self) -> bool {
        self.sidebar_intent.is_some()
    }

    /// Live, unsaved: updates the draft global and every HerdrWindow.
    pub(super) fn preview_sidebar(&mut self, mut intent: SidebarIntent, cx: &mut Context<Self>) {
        if self.quitting || self.closing.is_some() {
            return;
        }
        // A key stays touched for the draft's life, even dragged back.
        let changed = SidebarTouched::between(&self.config.sidebar_style, &intent.style);
        intent.touched.extend(&changed);
        if let Some(live) = &self.sidebar_intent {
            intent.touched.extend(&live.touched);
        }
        intent.apply(&mut self.config);
        let draft = cx.default_global::<SidebarDraft>();
        draft.active = true;
        draft.intent = Some(intent.clone());
        draft.revision = draft.revision.wrapping_add(1);
        self.sidebar_intent = Some(intent);
        self.broadcast_sidebar(cx);
    }

    /// Drops the live draft unwritten and shows the loaded values again.
    #[cfg(all(feature = "integration-test", feature = "mockup", target_os = "macos"))]
    pub(super) fn discard_sidebar_draft(&mut self, cx: &mut Context<Self>) {
        self.sidebar_intent = None;
        self.sidebar_queued = false;
        clear_sidebar_draft(cx);
        cx.default_global::<SidebarDraft>().intent = None;
        SidebarIntent::of(&self.loaded_config).apply(&mut self.config);
        self.broadcast_sidebar(cx);
    }

    /// Live and queued for save: preview, then write every changed key
    /// through one serial save.
    pub(super) fn commit_sidebar(&mut self, intent: SidebarIntent, cx: &mut Context<Self>) {
        if self.quitting || self.closing.is_some() {
            return;
        }
        self.preview_sidebar(intent, cx);
        self.sidebar_queued = true;
        self.save_sidebar_draft(cx);
    }

    pub(super) fn broadcast_sidebar(&self, cx: &mut Context<Self>) {
        let intent = SidebarIntent::of(&self.config);
        for handle in cx.windows() {
            if let Some(handle) = handle.downcast::<crate::HerdrWindow>() {
                let _ = handle.update(cx, |view, _, cx| {
                    intent.apply(&mut view.config);
                    cx.notify();
                });
            }
        }
        // Windows opened later start from the draft too.
        let mut appearance = cx
            .try_global::<crate::app::InitialAppearance>()
            .cloned()
            .unwrap_or_default();
        intent.apply(&mut appearance.config);
        cx.set_global(appearance);
        cx.notify();
    }

    fn sidebar_operation(
        &self,
        intent: &SidebarIntent,
    ) -> impl FnOnce() -> crate::Result<()> + Send + 'static + use<> {
        self.sidebar_write(diff_preferences(&self.loaded_config, intent))
    }

    fn sidebar_write(
        &self,
        edits: Vec<Preference>,
    ) -> impl FnOnce() -> crate::Result<()> + Send + 'static + use<> {
        #[cfg(test)]
        let io = self.controls.preference_io.clone();
        move || {
            #[cfg(test)]
            if let Some(io) = io {
                return edits.into_iter().try_for_each(|edit| (io.write)(edit));
            }
            edits.into_iter().try_for_each(Config::save_preference)
        }
    }

    /// Writes the draft unless another save is running; that save's
    /// completion resumes a queued draft through `finish_close`.
    pub(super) fn save_sidebar_draft(&mut self, cx: &mut Context<Self>) {
        let Some(intent) = self.sidebar_intent.clone() else {
            self.sidebar_queued = false;
            return;
        };
        if self.busy() {
            return;
        }
        self.sidebar_queued = false;
        if diff_preferences(&self.loaded_config, &intent).is_empty() {
            // Back where the file already is: nothing to write.
            self.sidebar_intent = None;
            clear_sidebar_draft(cx);
            self.finish_close(cx);
            cx.notify();
            return;
        }
        let operation = self.sidebar_operation(&intent);
        self.sidebar_saving = Some(intent);
        #[cfg(test)]
        if let Some(io) = self.controls.preference_io.clone() {
            self.save_with(operation, move || (io.load)(), false, cx);
            return;
        }
        self.save_native(operation, cx);
    }

    /// Called by the save completion with the intent that save wrote. A newer
    /// draft made during the write stays live and queued.
    pub(super) fn finish_sidebar_save(
        &mut self,
        saved: Option<SidebarIntent>,
        ok: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(saved) = saved else {
            return false;
        };
        if !ok {
            return false;
        }
        // A newer draft stays live; only a commit (which sets
        // `sidebar_queued`) writes it, never a preview mid-drag.
        if self.sidebar_intent.as_ref() == Some(&saved) {
            self.sidebar_intent = None;
            clear_sidebar_draft(cx);
        }
        true
    }

    /// The write a quit still owes, unless the running save already
    /// writes this very draft. The file may hold the running save's values
    /// by then, so the quit writes every touched key rather than a diff
    /// against the last load: a revert to the loaded value must still land.
    /// A draft's touched keys include every earlier version's, so a host a
    /// running save added and the draft dropped is cleared too.
    pub(super) fn take_shutdown_sidebar(
        &mut self,
    ) -> Option<impl FnOnce() -> crate::Result<()> + Send + 'static + use<>> {
        let intent = self.sidebar_intent.take()?;
        if self.sidebar_saving.as_ref() == Some(&intent) {
            return None;
        }
        Some(self.sidebar_write(touched_preferences(&intent)))
    }

    /// After a load: the live draft keeps only its touched keys over what
    /// was read, and `config` shows the result.
    pub(super) fn rebase_sidebar_draft(&mut self) {
        if let Some(intent) = &mut self.sidebar_intent {
            intent.rebase(&self.config);
            intent.overlay(&mut self.config);
        }
    }

    /// One row per host: connected endpoints first in window order, then
    /// configured hosts that are not connected, alphabetically. Each names
    /// the message hinting at its state, or is empty.
    pub(super) fn host_rows(&self, cx: &App) -> Vec<(String, Option<u32>, &'static str)> {
        let hosts = &self.config.sidebar_style.hosts;
        let mut rows: Vec<(String, Option<u32>, &'static str)> = Vec::new();
        if let Some(source) = self.source.upgrade() {
            for endpoint in &source.read(cx).endpoints {
                if !endpoint.live.status.is_connected()
                    || rows.iter().any(|(label, ..)| *label == endpoint.label)
                {
                    continue;
                }
                let color = hosts.get(&endpoint.label).copied();
                let hint = if color.is_some() {
                    ""
                } else {
                    "settings-sidebar-host-uncoloured"
                };
                rows.push((endpoint.label.clone(), color, hint));
            }
        }
        // The map is sorted, so configured-only hosts come alphabetically.
        for (host, color) in hosts {
            if !rows.iter().any(|(label, ..)| label == host) {
                rows.push((host.clone(), Some(*color), "settings-sidebar-host-offline"));
            }
        }
        rows
    }

    pub(super) fn set_host_color(
        &mut self,
        host: &str,
        color: Option<u32>,
        cx: &mut Context<Self>,
    ) {
        let mut intent = SidebarIntent::of(&self.config);
        match color {
            Some(color) => intent.style.hosts.insert(host.to_owned(), color),
            None => intent.style.hosts.remove(host),
        };
        self.commit_sidebar(intent, cx);
    }

    pub(super) fn begin_hex_edit(
        &mut self,
        host: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.quitting || self.closing.is_some() {
            return;
        }
        if !self.finish_hex_edit(true, cx) {
            self.finish_hex_edit(false, cx);
        }
        let current = self
            .host_rows(cx)
            .into_iter()
            .find(|(label, ..)| label == host)
            .and_then(|(_, color, _)| color)
            .map(|color| format!("{color:06x}"))
            .unwrap_or_default();
        let input = cx.new(SearchInput::new);
        input.update(cx, |input, cx| {
            input.set_text_selected(&current, cx);
            input.set_appearance(self.config.terminal.clone(), self.theme.clone(), cx);
        });
        let focus = input.read(cx).focus.clone();
        // Like Enter: an invalid colour keeps the field open and marked;
        // only Escape discards it.
        let blur = cx.on_blur(&focus, window, |this, _, cx| {
            this.finish_hex_edit(true, cx);
        });
        self.hex_edit = Some(HexEdit {
            host: host.to_owned(),
            input,
            invalid: false,
            _blur: blur,
        });
        window.focus(&focus, cx);
        cx.notify();
    }

    /// Commits or abandons the typed colour. False means the text is not a
    /// colour (or is mid-composition) and the field stays open, marked.
    pub(super) fn finish_hex_edit(&mut self, save: bool, cx: &mut Context<Self>) -> bool {
        let Some(edit) = &mut self.hex_edit else {
            return true;
        };
        let color = if save {
            if edit.input.read(cx).is_composing() {
                return false;
            }
            let Some(color) = ui::parse_hex_input(edit.input.read(cx).text()) else {
                edit.invalid = true;
                cx.notify();
                return false;
            };
            Some(color)
        } else {
            None
        };
        let host = std::mem::take(&mut edit.host);
        self.hex_edit = None;
        if let Some(color) = color {
            self.set_host_color(&host, Some(color), cx);
        }
        cx.notify();
        true
    }
}
