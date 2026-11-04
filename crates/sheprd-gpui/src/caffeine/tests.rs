#![allow(clippy::unwrap_used)]
use super::*;
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{Arc, Mutex},
    thread::ThreadId,
};

mod changes;
mod hidden_cup;
mod right_click;
mod scope;
mod toggle;
mod until_agents;
mod work;

/// Where a stand-in hold was made and released, in order.
#[derive(Clone, Default)]
struct Log(Arc<Mutex<Vec<(Command, ThreadId)>>>);

impl Log {
    fn push(&self, command: Command) {
        self.0
            .lock()
            .unwrap()
            .push((command, std::thread::current().id()));
    }

    fn entries(&self) -> Vec<(Command, ThreadId)> {
        self.0.lock().unwrap().clone()
    }
}

/// Stands in for `keepawake::KeepAwake`: dropping it is the release.
struct Held(Log);

impl Drop for Held {
    fn drop(&mut self) {
        self.0.push(Command::Release);
    }
}

fn holder(log: &Log) -> impl FnMut(AwakeScope) -> keepawake::Result<Held> + Send + 'static {
    let log = log.clone();
    move |_| {
        log.push(Command::Hold);
        Ok(Held(log.clone()))
    }
}

/// Collects what `toggle` reports, for the test to inspect afterwards.
#[derive(Clone, Default)]
struct Reports(Rc<RefCell<Vec<crate::Error>>>);

impl Reports {
    fn sink(&self) -> impl FnOnce(crate::Error, &mut App) + 'static {
        let reports = self.0.clone();
        move |error, _| reports.borrow_mut().push(error)
    }

    fn take(&self) -> Vec<crate::Error> {
        self.0.take()
    }
}

fn cup_of(cx: &mut gpui::TestAppContext) -> Cup {
    cx.update(|cx| cup(cx))
}

/// A stand-in window that a test reports work for.
fn window(id: u64) -> WindowId {
    WindowId::from(id)
}

fn commands(log: &Log) -> Vec<Command> {
    log.entries().iter().map(|(command, _)| *command).collect()
}
