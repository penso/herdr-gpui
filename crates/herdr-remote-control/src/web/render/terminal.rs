//! A terminal tab on the phone, one pane at a time (mockup
//! `terminal.html`: two fixed key rows, an action sheet, text fitted to the
//! width). The screen region is the only one the stream replaces.

use super::{ansi::Screen, home::urlencode, icons};
use crate::hub::{Terminal, TerminalPane, Workspace};
use maud::{Markup, html};

/// The keys above the keyboard: what each sends, in Herdr's key syntax,
/// and its label. `ctrl` and `alt` are modifiers held for the next key.
const TOP_ROW: &[(&str, &str)] = &[
    ("esc", "esc"),
    ("tab", "⇥"),
    ("|", "|"),
    ("slash", "/"),
    ("minus", "-"),
    ("~", "~"),
    ("backtick", "`"),
];
const BOTTOM_ROW: &[(&str, &str)] = &[
    ("ctrl+d", "^D"),
    ("ctrl+z", "^Z"),
    ("ctrl+r", "^R"),
    ("ctrl+l", "^L"),
    ("ctrl+a", "^A"),
    ("ctrl+e", "^E"),
];
const ARROWS: &[(&str, &str)] = &[("left", "←"), ("up", "↑"), ("down", "↓"), ("right", "→")];

/// The actions the sheet offers on the pane and its tab.
/// A sheet tile: the action it sends, its icon, and its label.
type PaneAction = (&'static str, fn(u16) -> Markup, &'static str);

const PANE_ACTIONS: &[PaneAction] = &[
    ("split_right", icons::split_right, "Split right"),
    ("split_down", icons::split_down, "Split down"),
    ("zoom", icons::zoom, "Zoom"),
    ("close_pane", icons::close, "Close"),
];

/// The whole terminal fragment for `pane` in `terminal`.
pub(crate) fn terminal(
    workspace: &Workspace,
    terminal: &Terminal,
    pane: &TerminalPane,
    screen: &Screen,
) -> Markup {
    let index = terminal
        .panes
        .iter()
        .position(|candidate| candidate.pane == pane.pane)
        .unwrap_or(0);
    let previous = index
        .checked_sub(1)
        .and_then(|index| terminal.panes.get(index));
    let next = terminal.panes.get(index + 1);
    let place = pane.cwd.as_deref().map(super::display_folder);
    html! {
        div id="term" class="term-page" data-pane=(pane.pane) data-tab=(terminal.tab) x-data="terminal" {
            header class="flex items-center gap-2 px-3 pt-1.5 pb-1.5 border-b border-line bg-bg" {
                a href="/" data-nav class="grid place-items-center text-accent w-8 h-9 -ml-1.5 shrink-0" aria-label="Back" { (icons::chevron_left(28)) }
                span class="flex flex-col min-w-0 flex-1" {
                    b class="truncate" { (title(terminal, pane)) }
                    small class="text-muted truncate" {
                        (workspace.label)
                        @if terminal.panes.len() > 1 {
                            " · pane " (index + 1) " of " (terminal.panes.len())
                        }
                        @if let Some(place) = &place { " · " (place) }
                    }
                }
                // Paste reads the clipboard, text or a screenshot; a picture
                // is stored on this computer and its path typed into the pane.
                input type="file" id="term-file" accept="image/png,image/jpeg,image/gif,image/webp" class="hidden";
                button type="button" class="tbtn" data-term-picture aria-label="Send a picture" { (icons::picture(18)) }
                button type="button" class="tbtn" data-term-paste aria-label="Paste" { (icons::clipboard(18)) }
                button type="button" class="tbtn" data-term-zoom aria-label="Text size" { "Aa" }
                button type="button" class="tbtn herdr" x-on:click="openSheet" aria-label="Herdr actions" { "◆" }
            }
            @if terminal.panes.len() > 1 {
                nav class="flex items-center gap-1.5 px-3 py-1.5 overflow-x-auto" aria-label="Panes" {
                    @if let Some(previous) = previous {
                        a class="tchip" href={ "/term/" (urlencode(&previous.pane)) } data-nav aria-label="Previous pane" { "‹" }
                    }
                    @for (number, candidate) in terminal.panes.iter().enumerate() {
                        a class={ "tchip" @if candidate.pane == pane.pane { " on" } }
                          href={ "/term/" (urlencode(&candidate.pane)) } data-nav {
                            @match candidate.label.as_deref() {
                                Some(label) => { (number + 1) " " (label) }
                                None => { "Pane " (number + 1) }
                            }
                        }
                    }
                    @if let Some(next) = next {
                        a class="tchip" href={ "/term/" (urlencode(&next.pane)) } data-nav aria-label="Next pane" { "›" }
                    }
                }
            }
            (terminal_screen(screen))
            // Taps on the screen bring up the keyboard through this field;
            // what is typed goes to the pane key by key.
            textarea id="term-input" class="term-input" autocapitalize="off" autocomplete="off"
                autocorrect="off" spellcheck="false" enterkeyhint="send" aria-label="Type into the terminal" {}
            div class="tkeys" {
                div class="tkeys-row" {
                    button type="button" class="tkey" data-mod="ctrl" { "ctrl" }
                    button type="button" class="tkey" data-mod="alt" { "alt" }
                    @for (key, label) in TOP_ROW {
                        button type="button" class="tkey" data-key=(key) { (label) }
                    }
                    button type="button" class="tkey stop grow" data-key="ctrl+c" aria-label="Control C" { "^C" }
                }
                div class="tkeys-row" {
                    @for (key, label) in BOTTOM_ROW {
                        button type="button" class="tkey" data-key=(key) { (label) }
                    }
                    span class="grow" {}
                    @for (key, label) in ARROWS {
                        button type="button" class="tkey" data-key=(key) { (label) }
                    }
                    button type="button" class="tkey" data-term-keyboard aria-label="Keyboard" { "⌨" }
                }
            }
            div class="sheet-backdrop" x-cloak x-show="sheet" x-on:click="closeSheet" {}
            div class="tsheet" x-cloak x-show="sheet" {
                div class="grab" {}
                h4 { "This pane" }
                div class="tiles" {
                    @for (action, icon, label) in PANE_ACTIONS {
                        button type="button"
                            class={ "tile" @if *action == "close_pane" { " danger" } }
                            data-term-action=(action) {
                            (icon(26)) span { (label) }
                        }
                    }
                }
                div class="tsheet-list" {
                    button type="button" data-term-action="new_tab" { span { "New tab" } span class="text-muted" { (icons::plus(22)) } }
                    a href={ "/new-chat?workspace=" (urlencode(&workspace.id)) } data-nav { span { "New chat in this workspace" } span class="text-muted" { (icons::chat(22)) } }
                    button type="button" class="text-red" data-term-action="close_tab" { span { "Close tab" } span { (icons::close(22)) } }
                }
            }
        }
    }
}

/// The header's name: the pane's label, else the tab's, where Herdr's
/// default tab label (its number) reads as "Tab N".
fn title(terminal: &Terminal, pane: &TerminalPane) -> String {
    if let Some(label) = &pane.label {
        return label.clone();
    }
    if !terminal.label.is_empty() && terminal.label.bytes().all(|byte| byte.is_ascii_digit()) {
        return format!("Tab {}", terminal.label);
    }
    terminal.label.clone()
}

/// The screen region the stream replaces.
pub(crate) fn terminal_screen(screen: &Screen) -> Markup {
    html! {
        pre id="term-screen" class="term-screen" data-columns=(screen.columns) { (screen.markup) }
    }
}
