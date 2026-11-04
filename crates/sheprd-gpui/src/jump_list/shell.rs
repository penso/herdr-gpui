//! The jump list worker: saves the recent checkouts and hands the entries to
//! the shell through `ICustomDestinationList`. It runs on its own thread, in
//! a multithreaded COM apartment of its own, so the UI thread's apartment and
//! its message loop are never involved.
//!
//! The shell's COM interfaces have no safe wrapper, so the calls are
//! `unsafe`; each takes only interfaces created here and owned strings that
//! outlive the call.
#![allow(unsafe_code)]

use super::{Entries, Item, Mailbox, Saved};
use crate::{constants::APP_ID, link::Link, state_file};
use std::{
    path::Path,
    sync::{Mutex, mpsc},
};
use windows::{
    Win32::{
        Foundation::{E_ACCESSDENIED, PROPERTYKEY},
        System::Com::{
            CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
            CoUninitialize, StructuredStorage::PROPVARIANT,
        },
        UI::Shell::{
            Common::{IObjectArray, IObjectCollection},
            DestinationList, EnumerableObjectCollection, ICustomDestinationList, IShellLinkW,
            PropertiesSystem::IPropertyStore,
            ShellLink,
        },
    },
    core::{GUID, HSTRING, Interface},
};

/// `PKEY_Title`, which the jump list shows as an entry's name.
const TITLE: PROPERTYKEY = PROPERTYKEY {
    fmtid: GUID::from_u128(0xf29f85e0_4ff9_1068_ab91_08002b27b3d9),
    pid: 2,
};
/// The longest command line a shell link keeps (`INFOTIPSIZE`).
const MAX_ARGUMENTS: usize = 1024;

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error("the shell refused the jump list")]
    Shell(#[from] windows::core::Error),
}

/// Ends COM on the worker thread when it stops.
struct Apartment;

impl Drop for Apartment {
    fn drop(&mut self) {
        // SAFETY: paired with the successful `CoInitializeEx` in `run`, on
        // the same thread, after every interface here was released.
        unsafe { CoUninitialize() };
    }
}

pub(super) fn run(
    wake: &mpsc::Receiver<()>,
    mailbox: &Mutex<Mailbox>,
    path: Option<&Path>,
    exe: Option<&Path>,
) {
    let Some(exe) = exe else {
        tracing::warn!("The jump list is off: this executable was not found");
        return;
    };
    // SAFETY: called once on this thread before any COM use.
    if let Err(error) = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.ok() {
        tracing::warn!(%error, "The jump list is off");
        return;
    }
    let _apartment = Apartment;
    for () in wake {
        let Some((entries, mut recent)) = mailbox
            .lock()
            .ok()
            .and_then(|mut mailbox| mailbox.pending.take())
        else {
            continue;
        };
        let removed = match publish(&entries, exe) {
            Ok(removed) => removed,
            Err(error) => {
                tracing::warn!(error = ?error, "Cannot update the jump list; trying again later");
                if let Ok(mut mailbox) = mailbox.lock() {
                    mailbox.failed = true;
                }
                continue;
            }
        };
        if !removed.is_empty() {
            recent.retain(|entry| {
                !removed.contains(&Link::Open {
                    path: entry.path.clone(),
                })
            });
            if let Ok(mut mailbox) = mailbox.lock() {
                mailbox.removed.extend(removed);
            }
        }
        if let Some(path) = path
            && let Err(error) = state_file::write(path, &Saved { recent })
        {
            tracing::warn!(%error, "Cannot save the jump list");
        }
    }
}

/// Replaces the jump list, leaving out what the user removed from it since
/// the last one, as the shell requires; returns those links.
fn publish(entries: &Entries, exe: &Path) -> Result<Vec<Link>, Error> {
    // SAFETY: the documented class and interface pair, in this thread's
    // apartment.
    let list: ICustomDestinationList =
        unsafe { CoCreateInstance(&DestinationList, None, CLSCTX_INPROC_SERVER)? };
    // SAFETY: the ID outlives the call; it must match the process's and the
    // shortcuts' AppUserModelID for the list to show on that button.
    unsafe { list.SetAppID(&HSTRING::from(APP_ID))? };
    let mut slots = 0;
    // SAFETY: `slots` lives across the call; the list is committed or
    // abandoned (on drop) below.
    let removed: IObjectArray = unsafe { list.BeginList(&mut slots)? };
    let removed = removed_links(&removed)?;
    let exe = HSTRING::from(exe.as_os_str());
    if let Some(recent) = collection(&exe, &entries.recent, &removed)? {
        // SAFETY: the name outlives the call.
        match unsafe { list.AppendCategory(&HSTRING::from("Recent workspaces"), &recent) } {
            Ok(()) => {}
            // The user turned off "Show recently opened items in Start, Jump
            // Lists, and File Explorer"; the shell then refuses categories,
            // and recent workspaces are left out as that setting asks.
            Err(error) if error.code() == E_ACCESSDENIED => {
                tracing::debug!("Recent items are turned off; the jump list lists devices only");
            }
            Err(error) => return Err(error.into()),
        }
    }
    // Tasks are shown whatever that setting is.
    if let Some(devices) = collection(&exe, &entries.devices, &removed)? {
        // SAFETY: a live object array.
        unsafe { list.AddUserTasks(&devices)? };
    }
    // SAFETY: finishes the list `BeginList` started.
    unsafe { list.CommitList()? };
    Ok(removed)
}

/// Shortcuts for the items the user has not removed, or `None` when none is
/// left.
fn collection(
    exe: &HSTRING,
    items: &[Item],
    removed: &[Link],
) -> windows::core::Result<Option<IObjectArray>> {
    // SAFETY: the documented class and interface pair, in this thread's
    // apartment.
    let collection: IObjectCollection =
        unsafe { CoCreateInstance(&EnumerableObjectCollection, None, CLSCTX_INPROC_SERVER)? };
    let mut added = false;
    for item in items.iter().filter(|item| !removed.contains(&item.link)) {
        let Some(link) = shell_link(exe, item)? else {
            continue;
        };
        // SAFETY: both interfaces are live; the collection keeps a reference.
        unsafe { collection.AddObject(&link)? };
        added = true;
    }
    added.then(|| collection.cast()).transpose()
}

/// A shortcut to the app (`exe`) with the item's link as its only argument,
/// or `None` for a link too long for a shortcut to keep.
fn shell_link(exe: &HSTRING, item: &Item) -> windows::core::Result<Option<IShellLinkW>> {
    // The link is percent-encoded, so quoting it keeps it one argument.
    let arguments = format!("\"{}\"", item.link.to_url());
    if arguments.len() >= MAX_ARGUMENTS {
        return Ok(None);
    }
    // SAFETY: the documented class and interface pair; every string passed
    // below outlives its call.
    unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
        link.SetPath(exe)?;
        link.SetArguments(&HSTRING::from(arguments))?;
        link.SetDescription(&HSTRING::from(item.link.to_url()))?;
        if matches!(item.link, Link::Open { .. }) {
            // The folder icon, as Explorer's own jump lists show.
            link.SetIconLocation(&HSTRING::from(r"%SystemRoot%\System32\shell32.dll"), 3)?;
        }
        let store: IPropertyStore = link.cast()?;
        store.SetValue(&TITLE, &PROPVARIANT::from(item.title.as_str()))?;
        store.Commit()?;
        Ok(Some(link))
    }
}

/// The links of the entries the user removed, read back from their
/// arguments. One that is not a link this app made is ignored.
fn removed_links(removed: &IObjectArray) -> windows::core::Result<Vec<Link>> {
    // SAFETY: a live array the shell returned.
    let count = unsafe { removed.GetCount()? };
    let mut links = Vec::new();
    for index in 0..count {
        // SAFETY: `index` is within the count just read.
        let Ok(link) = (unsafe { removed.GetAt::<IShellLinkW>(index) }) else {
            continue;
        };
        let mut buffer = [0u16; MAX_ARGUMENTS];
        // SAFETY: the buffer outlives the call, which writes at most its
        // length, NUL-terminated.
        if unsafe { link.GetArguments(&mut buffer) }.is_err() {
            continue;
        }
        let end = buffer
            .iter()
            .position(|unit| *unit == 0)
            .unwrap_or(buffer.len());
        let arguments = String::from_utf16_lossy(&buffer[..end]);
        if let Ok(parsed) = Link::parse(arguments.trim_matches('"')) {
            links.push(parsed);
        }
    }
    Ok(links)
}
