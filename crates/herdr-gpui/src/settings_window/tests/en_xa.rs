//! The `en-XA` leak test (`crate::i18n::leak_tests`) for every Settings
//! page: each draws only translated messages, around the section list and
//! footer, except for data such as theme and font names.
use super::*;
use crate::i18n::leak_tests::{assert_translated, draw, under_en_xa};
use gpui::{px, size};

/// Opens Settings on `section` under `en-XA` and checks every line drawn,
/// allowing the lines `data` lists.
fn check_page(section: Section, data: fn(&SettingsWindow, &App) -> Vec<String>) {
    under_en_xa("Settings en-XA leak", |cx, recorder| {
        let main = cx.add_window(crate::sidebar::layout_tests::fixture_window);
        let weak = cx.update(|cx| main.update(cx, |_, _, cx| cx.weak_entity()).unwrap());
        recorder.take();
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = SettingsWindow::new(weak, cx);
            view.section = section;
            window.focus(&view.focus, cx);
            view
        });
        cx.simulate_resize(size(px(1200.), px(3000.)));
        draw(cx);
        let data = cx.update(|_, cx| data(view.read(cx), cx));
        assert_translated(&format!("Settings > {section:?}"), &recorder.take(), &data);
    });
}

fn none(_: &SettingsWindow, _: &App) -> Vec<String> {
    Vec::new()
}

/// General: only messages.
fn general_data(_: &SettingsWindow, _: &App) -> Vec<String> {
    Vec::new()
}

/// Tabs & panes: each SSH host's name above its scrollback switch.
fn tabs_data(view: &SettingsWindow, _: &App) -> Vec<String> {
    view.remote_history
        .hosts()
        .iter()
        .map(|host| host.label.clone())
        .collect()
}

/// Configuration: the paths of both files.
fn configuration_data(view: &SettingsWindow, _: &App) -> Vec<String> {
    let shared = view
        .shared
        .as_ref()
        .map(|shared| shared.path.display().to_string());
    std::iter::once(view.controls.local_path.clone())
        .chain(shared)
        .collect()
}

/// The Plugins preview's example workspace and agent, drawn from the same
/// reports and rows as the page: names, as the sidebar's are, not messages.
/// The preview is narrow and truncates them, so each name's prefixes count.
fn plugin_preview(view: &SettingsWindow, cx: &App) -> Vec<String> {
    use crate::plugin_values::{
        PreviewPart, PreviewSource, preview_agent, preview_space, sidebar_values,
    };
    let reports = view
        .source
        .upgrade()
        .map(|source| source.read(cx).plugin_reports())
        .unwrap_or_default();
    let layout = &view.config.sidebar_layout;
    let values = sidebar_values(&reports, layout);
    let source = PreviewSource {
        values: &values,
        label_sample: reports
            .iter()
            .find_map(|report| report.label_sample.as_deref()),
        machine: (reports.len() > 1).then(|| reports[0].host.as_str()),
    };
    preview_space(&layout.spaces, &source)
        .into_iter()
        .chain(preview_agent(&layout.agents, &source))
        .flatten()
        .filter_map(|part| match part {
            PreviewPart::Text(text, ..) => Some(text),
            PreviewPart::StateIcon => None,
        })
        .flat_map(|name| {
            name.char_indices()
                .map(|(at, _)| name[..at].to_owned())
                .skip(1)
                .chain([name.clone()])
                .collect::<Vec<_>>()
        })
        .collect()
}

/// The theme names on the cards, and their glyph sample.
fn theme_page(view: &SettingsWindow, _: &App) -> Vec<String> {
    let names = view
        .themes
        .filtered
        .iter()
        .map(|choice| choice.name.clone());
    names.chain(["Aa".into()]).collect()
}

/// The hosts with a colour row, and the spacing numbers.
fn sidebar_page(view: &SettingsWindow, cx: &App) -> Vec<String> {
    let hosts = view.host_rows(cx).into_iter().map(|(host, ..)| host);
    hosts.collect()
}

/// Each face's family, its size, and the terminal's specimen, which is
/// code.
fn fonts_page(view: &SettingsWindow, _: &App) -> Vec<String> {
    let families =
        controls::FACES.map(|(face, _)| controls::face_font(&view.config, face).family.to_string());
    let specimen = [
        "let answer = 42;",
        "if answer != 0 {",
        "println!(\"ready\");",
        "0O 1lI {} [] != <=",
    ];
    families
        .into_iter()
        .chain(specimen.map(String::from))
        .collect()
}

/// Cloud devices on its first provider's tab: the provider names on the
/// tabs; `cloud_devices/tests/en_xa.rs` opens every card.
#[cfg(feature = "cloud")]
fn cloud_devices_data(_: &SettingsWindow, _: &App) -> Vec<String> {
    crate::cloud::CloudProvider::ALL
        .iter()
        .map(|&provider| crate::cloud::name(provider).to_owned())
        .collect()
}

macro_rules! pages {
    ($($(#[$attr:meta])* $name:ident: $section:ident, $data:expr;)+) => {
        $(
            $(#[$attr])*
            #[test]
            fn $name() {
                check_page(Section::$section, $data);
            }
        )+
    };
}

pages! {
    general: General, general_data;
    theme: Theme, theme_page;
    sidebar: Sidebar, sidebar_page;
    indicators: Indicators, none;
    fonts: Fonts, fonts_page;
    status_bar: StatusBar, none;
    tabs: Tabs, tabs_data;
    code: Code, none;
    notifications: Notifications, none;
    integrations: Integrations, none;
    plugins: Plugins, plugin_preview;
    #[cfg(feature = "cloud")]
    cloud_devices: CloudDevices, cloud_devices_data;
    remote_control: RemoteControl, none;
    configuration: Configuration, configuration_data;
}
