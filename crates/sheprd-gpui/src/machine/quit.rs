//! Saving machines as the app quits.
//!
//! GPUI gives its quit handlers 200 ms, and writing a machine's memory takes
//! seconds. Every way out passes through AppKit's
//! `applicationShouldTerminate:`, which GPUI's delegate does not answer and
//! offers no hook for, so this adds the method to the delegate's class.
//!
//! How to wait depends on who asked, because of where the question runs:
//!
//! - GPUI's own quit (the menu, the keys, `app quit`, closing the last window,
//!   an update's restart) calls `terminate:` from a block on the main queue.
//!   "Terminate later" would spin an event loop inside that block, and the
//!   serial queue would never reach the machine's completion handlers, so the
//!   app would hang. So the quit is cancelled, and tried again once the
//!   machines are saved.
//! - Anyone else (the Dock, logging out, shutting down) asks through an Apple
//!   Event, outside any block. Cancelling would abort a logout, so these
//!   "terminate later", and the event loop delivers the handlers.
//!
//! The method and the runtime call that adds it need `unsafe`, as neither
//! `objc2` nor `objc2-app-kit` has a safe form for adding to a class they did
//! not define.
#![allow(unsafe_code)]

use super::vm::{self, Status};
use gpui::AsyncApp;
use objc2::{
    MainThreadMarker, ffi,
    runtime::{AnyClass, AnyObject, Imp, Sel},
    sel,
};
use objc2_app_kit::{NSApplication, NSApplicationTerminateReply};
use objc2_foundation::NSAppleEventManager;
use std::{
    cell::Cell,
    collections::HashSet,
    time::{Duration, Instant},
};

/// How often the quit looks at the saves, and how long it waits for them. A
/// save stuck past this loses that machine's state, not the user's quit.
pub(super) const POLL: Duration = Duration::from_millis(100);
const LIMIT: Duration = Duration::from_secs(120);

thread_local! {
    static INSTALLED: Cell<bool> = const { Cell::new(false) };
    /// Set while saves hold the quit back, so a second quit does not start
    /// a second wait.
    static WAITING: Cell<bool> = const { Cell::new(false) };
    /// Set once the saves are over, so the quit goes through whatever became
    /// of them: a machine whose save failed must not hold the app forever.
    static LEAVING: Cell<bool> = const { Cell::new(false) };
}

/// Answers `applicationShouldTerminate:` from now on.
pub(super) fn install() {
    if INSTALLED.replace(true) {
        return;
    }
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let application = NSApplication::sharedApplication(mtm);
    let Some(delegate) = application.delegate() else {
        return;
    };
    let object: &AnyObject = (*delegate).as_ref();
    let class: &AnyClass = object.class();
    let answer: extern "C-unwind" fn(&AnyObject, Sel, &AnyObject) -> NSApplicationTerminateReply =
        should_terminate;
    // SAFETY: `answer` has the method's signature, which `types` spells: an
    // NSUInteger from self, the selector, and the sender. A class that has
    // the method already keeps its own.
    unsafe {
        let imp: Imp = std::mem::transmute(answer);
        let types = c"Q@:@";
        ffi::class_addMethod(
            std::ptr::from_ref(class).cast_mut(),
            sel!(applicationShouldTerminate:),
            imp,
            types.as_ptr(),
        );
    }
    // AppKit may note what a delegate answers as it is set.
    application.setDelegate(Some(&delegate));
}

extern "C-unwind" fn should_terminate(
    _: &AnyObject,
    _: Sel,
    _: &AnyObject,
) -> NSApplicationTerminateReply {
    let running = vm::running();
    let app = vm::app().filter(|_| !running.is_empty() && !LEAVING.get());
    let Some(app) = app else {
        return NSApplicationTerminateReply::TerminateNow;
    };
    let event = NSAppleEventManager::sharedAppleEventManager().currentAppleEvent();
    let way = if event.is_some() {
        Way::Reply
    } else {
        Way::Again
    };
    if !WAITING.replace(true) {
        vm::close();
        wait(app, running, way);
    }
    match way {
        Way::Reply => NSApplicationTerminateReply::TerminateLater,
        Way::Again => NSApplicationTerminateReply::TerminateCancel,
    }
}

/// How a quit held back for saves goes on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Way {
    /// AppKit is waiting for the answer.
    Reply,
    /// The quit was cancelled, so the app quits again.
    Again,
}

/// What the quit does about a machine it holds for, given whether it asked
/// it to save already.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Done,
    Suspend,
    Wait,
}

fn step(status: &Status, asked: bool) -> Step {
    match status {
        // A machine on its way up cannot save yet, and one resuming has
        // already given up its old state, so it is waited for.
        Status::Starting | Status::Saving => Step::Wait,
        Status::Running if !asked => Step::Suspend,
        Status::Running | Status::Stopped | Status::Failed(_) => Step::Done,
    }
}

/// Saves each machine of `names`, which [`vm::close`] keeps from growing, and
/// lets the app go once none is left to save. One whose save failed or
/// outlasted [`LIMIT`] loses its state rather than the user's quit.
fn wait(app: AsyncApp, names: Vec<super::Name>, way: Way) {
    let waiter = app.clone();
    app.spawn(async move |_: &mut AsyncApp| {
        let start = Instant::now();
        let mut asked = HashSet::new();
        while start.elapsed() < LIMIT {
            let mut held = false;
            for name in &names {
                match step(&vm::status(name), asked.contains(name)) {
                    Step::Done => {}
                    Step::Suspend => {
                        asked.insert(name.clone());
                        waiter.update(|cx| vm::suspend(name, cx));
                        held = true;
                    }
                    Step::Wait => held = true,
                }
            }
            if !held {
                break;
            }
            waiter.background_executor().timer(POLL).await;
        }
        WAITING.set(false);
        LEAVING.set(true);
        match way {
            Way::Again => waiter.update(|cx| cx.quit()),
            Way::Reply => {
                if let Some(mtm) = MainThreadMarker::new() {
                    NSApplication::sharedApplication(mtm).replyToApplicationShouldTerminate(true);
                }
            }
        }
    })
    .detach();
}

#[cfg(test)]
mod tests;
