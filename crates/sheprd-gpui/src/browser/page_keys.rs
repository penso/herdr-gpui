//! Keys for a page that holds the keyboard. macOS first offers every key
//! down to the window's views as a possible shortcut, and GPUI's view takes
//! that offer ahead of the page inside it: it runs the app's bindings, and it
//! sends non-printing keys such as the arrows through its own text input, so
//! the page received them mangled. A page drawn as a child view also refuses
//! the offer itself, so its scripts never saw Cmd keys at all.
//!
//! A local event monitor runs before any of that. While a page is the
//! window's first responder, it hands the key straight to the page, so the
//! page's own shortcuts win, as a terminal program's do in its pane. Three
//! kinds of key go elsewhere, as they do in Safari:
//!
//! - Cut, copy, and paste reach a page as the edit menu's actions, not as
//!   keys: WebKit fires the page's clipboard events only for those. The
//!   app's own Edit menu would send them to the terminal instead.
//! - The keys macOS keeps for the app and its windows, such as quit and
//!   hide, stay with the app. A child page swallows keys it leaves
//!   unhandled, so these would otherwise do nothing while it has focus.
//! - The keys bound to moving to another tab, workspace, or agent stay with
//!   the app, as a browser keeps its tab keys. Without them, only the mouse
//!   could leave a page. So do the keys bound to the font size commands,
//!   which zoom the page, as a browser keeps its zoom keys.
//!
//! The monitor and the edit actions are the AppKit calls here, and they need
//! `unsafe`: the monitor's handler must return a valid event or null, and an
//! action must be a selector the page implements, neither of which
//! `objc2-app-kit` can check. wry and GPUI offer no hook ahead of key
//! equivalents.
#![allow(unsafe_code)]

use crate::{controls::Command, keymap::Keymap};
use block2::RcBlock;
use gpui::{KeybindingKeystroke, Keystroke, Modifiers};
use objc2::{
    ClassType, MainThreadMarker, rc::Retained, runtime::AnyObject, runtime::NSObjectProtocol, sel,
};
use objc2_app_kit::{NSEvent, NSEventMask, NSEventModifierFlags, NSEventType};
use objc2_web_kit::WKWebView;
use std::{
    cell::{OnceCell, RefCell},
    ptr::{NonNull, null_mut},
};

thread_local! {
    /// The monitor, installed with the first page and kept for the life of
    /// the app.
    static MONITOR: OnceCell<Option<Retained<AnyObject>>> = const { OnceCell::new() };
    /// The keystrokes of the commands that leave a page, from the keymap
    /// bound last.
    static LEAVING: RefCell<Vec<KeybindingKeystroke>> = const { RefCell::new(Vec::new()) };
}

/// The commands a focused page leaves to the app: moving to another tab,
/// workspace, or agent, moving the page's own tab, and the font size
/// commands, which zoom the page.
fn leaves_page(command: Command) -> bool {
    matches!(
        command,
        Command::NextTab
            | Command::PreviousTab
            | Command::TabNumber(_)
            | Command::MoveTabPrevious
            | Command::MoveTabNext
            | Command::NextWorkspace
            | Command::PreviousWorkspace
            | Command::WorkspaceNumber(_)
            | Command::NextAgent
            | Command::PreviousAgent
            | Command::AgentNumber(_)
            | Command::IncreaseFontSize
            | Command::DecreaseFontSize
            | Command::ResetFontSize
    )
}

/// From now on, a focused page leaves to the app the keystrokes `keymap`
/// binds to the commands that leave a page. Prefix chords are not among
/// them: the prefix is the page's.
pub(crate) fn bind(keymap: &Keymap) {
    let keys = keymap
        .bindings()
        .filter(|(command, _)| leaves_page(*command))
        .filter_map(|(_, label)| Keystroke::parse(label).ok())
        .map(KeybindingKeystroke::from_keystroke)
        .collect();
    LEAVING.with(|leaving| *leaving.borrow_mut() = keys);
}

/// Whether `typed` is bound to a command that leaves a page.
fn leaves(typed: &Keystroke) -> bool {
    LEAVING.with(|leaving| {
        leaving
            .borrow()
            .iter()
            .any(|bound| typed.should_match(bound))
    })
}

/// Starts routing keys to pages. Later calls do nothing.
pub(super) fn install() {
    MONITOR.with(|monitor| {
        monitor.get_or_init(|| {
            let handler = RcBlock::new(|event: NonNull<NSEvent>| -> *mut NSEvent {
                // SAFETY: AppKit passes a valid event that outlives this call.
                let native = unsafe { event.as_ref() };
                if deliver(native) {
                    null_mut()
                } else {
                    event.as_ptr()
                }
            });
            // SAFETY: the handler returns either the event AppKit gave it,
            // which AppKit keeps alive, or null, which drops the event.
            unsafe {
                NSEvent::addLocalMonitorForEventsMatchingMask_handler(
                    NSEventMask::KeyDown | NSEventMask::KeyUp,
                    &handler,
                )
            }
        });
    });
}

/// Where a key goes while a page holds the keyboard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Route {
    /// To the page, as a key.
    Page,
    /// To the page, as the edit menu's action.
    Edit(Edit),
    /// On to the app, as if no page were there.
    App,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Edit {
    Cut,
    Copy,
    Paste,
}

/// The modifiers a key was pressed with, as far as routing cares.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Held {
    command: bool,
    shift: bool,
    option: bool,
    control: bool,
    function: bool,
}

/// The keystroke GPUI reads from a key pressed with `held`. `plain` is what
/// the key types with no modifier, or with Command alone while Command is
/// held, as GPUI reads shortcuts; `shifted` is what it types with Shift as
/// held.
fn typed(held: Held, plain: &str, shifted: &str) -> Keystroke {
    let first = plain.chars().next();
    let named = first.and_then(named_key);
    Keystroke {
        modifiers: Modifiers {
            control: held.control,
            alt: held.option,
            shift: held.shift,
            platform: held.command,
            // Arrows and function keys always carry Fn, which GPUI drops.
            function: held.function
                && !first.is_some_and(|key| ('\u{f700}'..='\u{f747}').contains(&key)),
        },
        key_char: named.is_none().then(|| shifted.to_owned()),
        key: named.unwrap_or_else(|| plain.to_lowercase()),
    }
}

/// GPUI's name for a key that types no text of its own.
fn named_key(key: char) -> Option<String> {
    let name = match key {
        ' ' => "space",
        '\t' | '\u{19}' => "tab",
        '\r' | '\u{3}' => "enter",
        '\u{7f}' => "backspace",
        '\u{1b}' => "escape",
        '\u{f700}' => "up",
        '\u{f701}' => "down",
        '\u{f702}' => "left",
        '\u{f703}' => "right",
        '\u{f704}'..='\u{f726}' => return Some(format!("f{}", u32::from(key) - 0xf703)),
        '\u{f728}' => "delete",
        '\u{f729}' => "home",
        '\u{f72b}' => "end",
        '\u{f72c}' => "pageup",
        '\u{f72d}' => "pagedown",
        '\u{f746}' => "insert",
        _ => return None,
    };
    Some(name.to_owned())
}

/// Routes a key down by its modifiers and the character it types without
/// them. Key ups always follow to the page.
fn route(held: Held, key: &str) -> Route {
    let only_command = held.command && !held.shift && !held.option && !held.control;
    if only_command {
        match key {
            "x" => return Route::Edit(Edit::Cut),
            "c" => return Route::Edit(Edit::Copy),
            "v" => return Route::Edit(Edit::Paste),
            // Quit, hide, and minimize.
            "q" | "h" | "m" => return Route::App,
            _ => {}
        }
    }
    // Hide the other apps, and cycle the app's windows either way.
    let app_key = held.command
        && !held.control
        && match key {
            "h" => held.option && !held.shift,
            "`" | "~" => !held.option,
            _ => false,
        };
    if app_key { Route::App } else { Route::Page }
}

/// Hands `event` to the page holding its window's keyboard, if a page does.
/// Returns whether it did.
fn deliver(event: &NSEvent) -> bool {
    let Some(mtm) = MainThreadMarker::new() else {
        return false;
    };
    let Some(page) = event
        .window(mtm)
        .and_then(|window| window.firstResponder())
        .filter(|responder| responder.isKindOfClass(WKWebView::class()))
    else {
        return false;
    };
    match event.r#type() {
        NSEventType::KeyDown => {
            let flags = event.modifierFlags();
            let held = Held {
                command: flags.contains(NSEventModifierFlags::Command),
                shift: flags.contains(NSEventModifierFlags::Shift),
                option: flags.contains(NSEventModifierFlags::Option),
                control: flags.contains(NSEventModifierFlags::Control),
                function: flags.contains(NSEventModifierFlags::Function),
            };
            let shifted = event
                .charactersIgnoringModifiers()
                .map(|key| key.to_string())
                .unwrap_or_default();
            let layout = if held.command {
                NSEventModifierFlags::Command
            } else {
                NSEventModifierFlags::empty()
            };
            let plain = event
                .charactersByApplyingModifiers(layout)
                .map(|key| key.to_string())
                .unwrap_or_default();
            if leaves(&typed(held, &plain, &shifted)) {
                return false;
            }
            match route(held, &shifted.to_lowercase()) {
                Route::Page => page.keyDown(event),
                Route::App => return false,
                Route::Edit(edit) => {
                    let action = match edit {
                        Edit::Cut => sel!(cut:),
                        Edit::Copy => sel!(copy:),
                        Edit::Paste => sel!(paste:),
                    };
                    // SAFETY: WebKit's view implements the standard edit
                    // actions, which take an optional sender.
                    unsafe { page.tryToPerform_with(action, None) };
                }
            }
        }
        NSEventType::KeyUp => page.keyUp(event),
        _ => return false,
    }
    true
}

#[cfg(test)]
mod tests;
