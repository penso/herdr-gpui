//! The virtual Macs this app runs, on macOS.
//!
//! A machine belongs to the app, not to a window or a tab: its tab can close
//! or move to another window while it runs. Apple's view must share a process
//! with the machine, and both live on the main queue, so the state here is
//! the main thread's own. [`super::quit`] reaches it from AppKit, where no
//! `App` is at hand.
//!
//! `objc2-virtualization` marks every call `unsafe`, as it does not check
//! the framework's threading rules. Each call here runs on the main thread,
//! the machine's queue: the view's `MainThreadMarker` proves it, and the
//! machines are reached only through `MACHINES`, which is that thread's.
#![allow(unsafe_code)]

use super::{
    Display, Error, Name, Result,
    configuration::{Bundle, file_url},
    quit, store,
};
use block2::RcBlock;
use gpui::{App, AsyncApp};
use objc2::{
    AnyThread as _, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send,
    rc::Retained,
    runtime::{NSObject, NSObjectProtocol, ProtocolObject},
};
use objc2_foundation::NSError;
use objc2_virtualization::{VZVirtualMachine, VZVirtualMachineDelegate, VZVirtualMachineView};
use std::{
    cell::{Cell, OnceCell, RefCell},
    collections::HashMap,
    path::PathBuf,
    ptr::NonNull,
    rc::Rc,
};

type View = Retained<VZVirtualMachineView>;

define_class!(
    /// Hears a machine stop without being asked: the guest shut itself down,
    /// or the machine failed.
    // SAFETY: NSObject may be subclassed on the main thread, which
    // `MainThreadOnly` holds every use to, and `Watcher` has no `Drop`.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "SheprdMachineWatcher"]
    #[ivars = Name]
    struct Watcher;

    unsafe impl NSObjectProtocol for Watcher {}

    unsafe impl VZVirtualMachineDelegate for Watcher {
        #[unsafe(method(guestDidStopVirtualMachine:))]
        fn guest_did_stop(&self, _: &VZVirtualMachine) {
            self.stopped(Phase::Stopped);
        }

        #[unsafe(method(virtualMachine:didStopWithError:))]
        fn did_stop(&self, _: &VZVirtualMachine, error: &NSError) {
            self.stopped(Phase::Failed(description(error)));
        }
    }
);

impl Watcher {
    fn new(name: Name, mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(name);
        // SAFETY: NSObject's initializer, on an instance with its ivars set.
        unsafe { msg_send![super(this), init] }
    }

    fn stopped(&self, phase: Phase) {
        let name = self.ivars().clone();
        if let Some(app) = app() {
            app.update(|cx| set_phase(&name, phase, cx));
        }
    }
}

/// A machine that exists, and what must stay alive with it: the framework
/// holds its delegate weakly.
#[derive(Clone)]
struct Live {
    machine: Retained<VZVirtualMachine>,
    _watcher: Retained<Watcher>,
    /// Whether its display follows the tab.
    fit: bool,
}

enum Phase {
    Stopped,
    /// Files are being read, or the machine is booting or resuming.
    Starting(Option<Live>),
    Running(Live),
    /// Its state is being written; it stops once that is done.
    Saving(Live),
    Failed(String),
}

impl Phase {
    fn live(&self) -> Option<&Live> {
        match self {
            Self::Running(live) | Self::Saving(live) | Self::Starting(Some(live)) => Some(live),
            Self::Stopped | Self::Starting(None) | Self::Failed(_) => None,
        }
    }
}

/// What a machine is doing, for the window to show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Status {
    Stopped,
    Starting,
    Running,
    Saving,
    Failed(String),
}

struct Entry {
    phase: Phase,
    /// Every tab's view on it, each shown it once it runs.
    views: Vec<View>,
    /// Its files are going, so nothing may start it.
    deleting: bool,
}

impl Default for Entry {
    fn default() -> Self {
        Self {
            phase: Phase::Stopped,
            views: Vec::new(),
            deleting: false,
        }
    }
}

thread_local! {
    static MACHINES: RefCell<HashMap<Name, Entry>> = RefCell::default();
    /// For the framework's callbacks, which arrive with no `App` at hand.
    static APP: OnceCell<AsyncApp> = const { OnceCell::new() };
    /// Set as the app quits, so no machine starts that the quit does not know.
    static CLOSED: Cell<bool> = const { Cell::new(false) };
}

/// Starts no machine from now on: the app is quitting.
pub(super) fn close() {
    CLOSED.set(true);
}

/// Whether machine `name` may start.
fn startable(name: &Name) -> bool {
    !CLOSED.get() && !with(name, |machine| machine.deleting)
}

pub(super) fn app() -> Option<AsyncApp> {
    APP.with(|app| app.get().cloned())
}

fn with<T>(name: &Name, read: impl FnOnce(&mut Entry) -> T) -> T {
    MACHINES.with(|machines| read(machines.borrow_mut().entry(name.clone()).or_default()))
}

/// What machine `name` is doing.
pub(crate) fn status(name: &Name) -> Status {
    MACHINES.with(
        |machines| match machines.borrow().get(name).map(|m| &m.phase) {
            None | Some(Phase::Stopped) => Status::Stopped,
            Some(Phase::Starting(_)) => Status::Starting,
            Some(Phase::Running(_)) => Status::Running,
            Some(Phase::Saving(_)) => Status::Saving,
            Some(Phase::Failed(message)) => Status::Failed(message.clone()),
        },
    )
}

/// The machines taking this Mac's memory and CPU, whether a tab shows them
/// or not, by name.
pub(crate) fn running() -> Vec<Name> {
    let mut names: Vec<Name> = MACHINES.with(|machines| {
        machines
            .borrow()
            .iter()
            .filter(|(_, machine)| {
                matches!(
                    machine.phase,
                    Phase::Starting(_) | Phase::Running(_) | Phase::Saving(_)
                )
            })
            .map(|(name, _)| name.clone())
            .collect()
    });
    names.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    names
}

/// A new view on machine `name`, started if it is not running.
pub(crate) fn view(name: &Name, cx: &mut App) -> Result<View> {
    let mtm = MainThreadMarker::new().ok_or(Error::Unsupported)?;
    // SAFETY: an unattached view, made and set up on the main thread. The
    // host's own Cmd-Tab and Spotlight stay the host's.
    let view = unsafe {
        let view = VZVirtualMachineView::new(mtm);
        view.setCapturesSystemKeys(false);
        view
    };
    with(name, |machine| {
        if let Some(live) = machine.phase.live() {
            attach(&view, live);
        }
        machine.views.push(view.clone());
    });
    start(name, cx);
    Ok(view)
}

/// Forgets `view`, whose tab closed. The machine goes on running.
pub(crate) fn release(view: &VZVirtualMachineView) {
    MACHINES.with(|machines| {
        for machine in machines.borrow_mut().values_mut() {
            machine.views.retain(|held| !std::ptr::eq(&**held, view));
        }
    });
    // SAFETY: on the main thread, where the view lives.
    unsafe { view.setVirtualMachine(None) };
}

fn attach(view: &VZVirtualMachineView, live: &Live) {
    // SAFETY: the view and the machine are both on the main thread.
    unsafe {
        view.setAutomaticallyReconfiguresDisplay(live.fit);
        view.setVirtualMachine(Some(&live.machine));
    }
}

/// Records what machine `name` does now, and redraws what shows it. A machine
/// that stopped leaves its views.
fn set_phase(name: &Name, phase: Phase, cx: &mut App) {
    with(name, |machine| {
        if machine.phase.live().is_some() && phase.live().is_none() {
            for view in &machine.views {
                // SAFETY: on the main thread, where the view lives.
                unsafe { view.setVirtualMachine(None) };
            }
        }
        machine.phase = phase;
    });
    cx.refresh_windows();
}

fn fail(name: &Name, error: &Error, cx: &mut App) {
    tracing::warn!(%name, %error, "Machine failed");
    let message = crate::i18n::Localize::localize(error);
    set_phase(name, Phase::Failed(message), cx);
}

/// A completion handler that calls `done` once, with what went wrong.
fn once(
    app: AsyncApp,
    done: impl FnOnce(Option<String>, &mut App) + 'static,
) -> RcBlock<dyn Fn(*mut NSError)> {
    let done = Cell::new(Some(done));
    RcBlock::new(move |error: *mut NSError| {
        // SAFETY: Virtualization passes null or an error that outlives this
        // call.
        let message = NonNull::new(error).map(|error| description(unsafe { error.as_ref() }));
        if let Some(done) = done.take() {
            app.update(|cx| done(message, cx));
        }
    })
}

/// Starts machine `name` unless it is already on its way, resuming it if it
/// was saved. A failure shows through [`status`].
pub(crate) fn start(name: &Name, cx: &mut App) {
    let free = with(name, |machine| {
        matches!(machine.phase, Phase::Stopped | Phase::Failed(_))
    });
    if !free || !startable(name) {
        return;
    }
    APP.with(|app| {
        app.get_or_init(|| cx.to_async());
    });
    quit::install();
    set_phase(name, Phase::Starting(None), cx);
    let name = name.clone();
    let read = cx.background_executor().spawn({
        let name = name.clone();
        async move { Bundle::read(&name) }
    });
    cx.spawn(async move |cx: &mut AsyncApp| {
        let bundle = read.await;
        cx.update(|cx| {
            // A quit or a delete came while the files were read. Nothing runs
            // yet, so the start just ends here.
            if !startable(&name) {
                return set_phase(&name, Phase::Stopped, cx);
            }
            if let Err(error) = bundle.and_then(|bundle| build(&name, &bundle, cx)) {
                fail(&name, &error, cx);
            }
        });
    })
    .detach();
}

/// Builds machine `name` from `bundle`, shows it in the views waiting for it,
/// and boots or resumes it.
fn build(name: &Name, bundle: &Bundle, cx: &mut App) -> Result<()> {
    let mtm = MainThreadMarker::new().ok_or(Error::Unsupported)?;
    // SAFETY: a class property.
    if !unsafe { VZVirtualMachine::isSupported() } {
        return Err(Error::Unsupported);
    }
    let configuration = bundle.configuration()?;
    // SAFETY: on the main thread, so the machine's queue is the main queue,
    // which Apple's view requires. The configuration was validated.
    let machine = unsafe {
        VZVirtualMachine::initWithConfiguration(VZVirtualMachine::alloc(), &configuration)
    };
    let watcher = Watcher::new(name.clone(), mtm);
    // SAFETY: `Live` keeps the delegate, which the machine holds weakly.
    unsafe { machine.setDelegate(Some(ProtocolObject::from_ref(&*watcher))) };
    let live = Live {
        machine,
        _watcher: watcher,
        fit: bundle.settings.display == Display::Fit,
    };
    with(name, |machine| {
        for view in &machine.views {
            attach(view, &live);
        }
    });
    set_phase(name, Phase::Starting(Some(live.clone())), cx);
    match &bundle.saved {
        Some(state) => restore(name, live, state.clone(), cx),
        None => boot(name, live, cx),
    }
    Ok(())
}

fn boot(name: &Name, live: Live, cx: &mut App) {
    let machine = live.machine.clone();
    let done = once(cx.to_async(), started(name.clone(), live));
    // SAFETY: on the machine's queue; the framework copies the block.
    unsafe { machine.startWithCompletionHandler(&done) };
}

/// What ends a start or a resume.
fn started(name: Name, live: Live) -> impl FnOnce(Option<String>, &mut App) + 'static {
    move |error, cx| match error {
        None => set_phase(&name, Phase::Running(live), cx),
        Some(message) => fail(&name, &Error::Framework(message), cx),
    }
}

/// Removes machine `name`'s saved state off the main thread, then calls `then`.
fn discard(name: Name, cx: &mut App, then: impl FnOnce(Name, Result<()>, &mut App) + 'static) {
    let removal = cx.background_executor().spawn({
        let name = name.clone();
        async move { store::discard_state(&name) }
    });
    cx.spawn(async move |cx: &mut AsyncApp| {
        let removed = removal.await;
        cx.update(|cx| then(name, removed, cx));
    })
    .detach();
}

/// Gives up on a machine whose saved state cannot be removed. Its guest must
/// not run while that state remains: the next launch would resume it over a
/// disk that moved on. Stopped here the two still match.
fn abandon(name: Name, live: &Live, error: Error, cx: &mut App) {
    halt(live, cx, move |refusal, cx| {
        if let Some(message) = refusal {
            tracing::warn!(%name, %message, "Cannot stop a machine");
        }
        fail(&name, &error, cx);
    });
}

/// Resumes from `state`. The state goes before the guest runs: resumed twice,
/// it would no longer match the disk. One that cannot be read boots afresh,
/// as after a power cut.
fn restore(name: &Name, live: Live, state: PathBuf, cx: &mut App) {
    let machine = live.machine.clone();
    let url = file_url(&state);
    let name = name.clone();
    let done = once(cx.to_async(), move |error, cx| {
        // A quit or a delete came during the restore. The guest has not run,
        // so the state still matches the disk: it stays for the next launch.
        if !startable(&name) {
            return force_stop(&name, &live, cx);
        }
        discard(name, cx, move |name, removed, cx| {
            if let Err(removal) = removed {
                return abandon(name, &live, removal, cx);
            }
            match error {
                None => {
                    let machine = live.machine.clone();
                    let done = once(cx.to_async(), started(name, live));
                    // SAFETY: on the machine's queue; the framework copies
                    // the block.
                    unsafe { machine.resumeWithCompletionHandler(&done) };
                }
                Some(message) => {
                    tracing::warn!(%name, %message, "Cannot resume a machine; booting it");
                    boot(&name, live, cx);
                }
            }
        });
    });
    // SAFETY: on the machine's queue; the framework copies the block.
    unsafe { machine.restoreMachineStateFromURL_completionHandler(&url, &done) };
}

/// Saves machine `name` and stops it, so it next resumes where it was. One
/// that is not running has nothing to save. A failure leaves it running.
pub(crate) fn suspend(name: &Name, cx: &mut App) {
    let live = with(name, |machine| match &machine.phase {
        Phase::Running(live) => Some(live.clone()),
        _ => None,
    });
    let (Some(live), Ok(state)) = (live, store::state_file(name)) else {
        return;
    };
    set_phase(name, Phase::Saving(live.clone()), cx);
    let name = name.clone();
    let machine = live.machine.clone();
    let done = once(cx.to_async(), move |error, cx| match error {
        Some(message) => refused(&name, &live, &message, cx),
        None => save(name, live, &state, cx),
    });
    // SAFETY: on the machine's queue; the framework copies the block.
    unsafe { machine.pauseWithCompletionHandler(&done) };
}

/// Writes the paused machine to `state`, then stops it.
fn save(name: Name, live: Live, state: &std::path::Path, cx: &mut App) {
    let machine = live.machine.clone();
    let url = file_url(state);
    let done = once(cx.to_async(), move |error, cx| {
        let Some(message) = error else {
            return force_stop(&name, &live, cx);
        };
        // Half a state must not be resumed.
        discard(name, cx, move |name, removed, cx| {
            if let Err(removal) = removed {
                return abandon(name, &live, removal, cx);
            }
            let machine = live.machine.clone();
            let done = once(cx.to_async(), move |_, cx| {
                refused(&name, &live, &message, cx);
            });
            // SAFETY: on the machine's queue; the framework copies the block.
            unsafe { machine.resumeWithCompletionHandler(&done) };
        });
    });
    // SAFETY: on the machine's queue; the framework copies the block.
    unsafe { machine.saveMachineStateToURL_completionHandler(&url, &done) };
}

fn refused(name: &Name, live: &Live, message: &str, cx: &mut App) {
    tracing::warn!(%name, %message, "Cannot save a machine");
    set_phase(name, Phase::Running(live.clone()), cx);
}

/// Stops the machine at once, as a power cut would. `then` hears any refusal.
fn halt(live: &Live, cx: &mut App, then: impl FnOnce(Option<String>, &mut App) + 'static) {
    let done = once(cx.to_async(), then);
    // SAFETY: on the machine's queue; the framework copies the block.
    unsafe { live.machine.stopWithCompletionHandler(&done) };
}

/// Stops a machine whose state is saved.
fn force_stop(name: &Name, live: &Live, cx: &mut App) {
    let name = name.clone();
    halt(live, cx, move |error, cx| {
        if let Some(message) = error {
            tracing::warn!(%name, %message, "Cannot stop a machine");
        }
        set_phase(&name, Phase::Stopped, cx);
    });
}

/// Asks the guest to shut down, as its power button would. It may ask its
/// user first; the machine stops when it has, which [`Watcher`] hears.
pub(crate) fn shut_down(name: &Name) -> Result<()> {
    let live = with(name, |machine| machine.phase.live().cloned());
    let Some(live) = live else {
        return Ok(());
    };
    // SAFETY: on the machine's queue.
    unsafe { live.machine.requestStopWithError() }
        .map_err(|error| Error::Framework(description(&error)))
}

/// Stops machine `name` if it runs, then deletes everything it holds. Nothing
/// starts it meanwhile. `done` hears how that went.
pub(crate) fn delete(name: &Name, cx: &mut App, done: impl FnOnce(Result<()>, &mut App) + 'static) {
    with(name, |machine| machine.deleting = true);
    let name = name.clone();
    cx.spawn(async move |cx: &mut AsyncApp| {
        let removed = match stop_for_delete(&name, cx).await {
            Ok(()) => {
                let name = name.clone();
                cx.background_executor()
                    .spawn(async move { store::delete(&name) })
                    .await
            }
            Err(error) => Err(error),
        };
        cx.update(|cx| {
            if removed.is_ok() {
                MACHINES.with(|machines| machines.borrow_mut().remove(&name));
            } else {
                with(&name, |machine| machine.deleting = false);
            }
            cx.refresh_windows();
            done(removed, cx);
        });
    })
    .detach();
}

/// Sees machine `name` stopped, as its disk stays open until then. A start or
/// a save in progress is waited out: the framework refuses to stop a machine
/// midway through one, and their completions would write over the phase.
async fn stop_for_delete(name: &Name, cx: &mut AsyncApp) -> Result<()> {
    let refusal = Rc::new(Cell::new(None));
    loop {
        if let Some(message) = refusal.take() {
            return Err(Error::Framework(message));
        }
        let next = with(name, |machine| match &machine.phase {
            Phase::Stopped | Phase::Failed(_) => None,
            Phase::Running(live) => Some(Some(live.clone())),
            Phase::Starting(_) | Phase::Saving(_) => Some(None),
        });
        match next {
            None => return Ok(()),
            Some(Some(live)) => cx.update(|cx| {
                // Saving, so that this stop is asked for once.
                set_phase(name, Phase::Saving(live.clone()), cx);
                let name = name.clone();
                let refusal = refusal.clone();
                halt(&live.clone(), cx, move |error, cx| {
                    let phase = match error {
                        None => Phase::Stopped,
                        Some(_) => Phase::Running(live),
                    };
                    refusal.set(error);
                    set_phase(&name, phase, cx);
                });
            }),
            Some(None) => cx.background_executor().timer(quit::POLL).await,
        }
    }
}

pub(super) fn description(error: &NSError) -> String {
    error.localizedDescription().to_string()
}
