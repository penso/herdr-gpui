//! The one `herdr-nearby` helper the app runs, for whoever needs it: the
//! pairing dialog while its offer shows, and presence while remote control
//! runs with `[remote] presence` on. It starts when the first needs it and
//! stops when the last lets go. Each phone's `pair` frame goes to the
//! daemon's control socket and back; `presence` frames go through
//! [`PresenceRelay`], which also decides when a device is near.

use super::{
    Event, Helper, Radio, STOP, advertise_presence, answer, frame, helper_path,
    presence::{PresenceRelay, Received},
    presence_frame, refusal, request,
};
use crate::remote::supervisor::{Client, Supervisor};
use gpui::{App, Global};
use std::{
    rc::{Rc, Weak},
    sync::mpsc::{self, TryRecvError},
    time::{Duration, Instant},
};

/// How often the station looks for a phone's frame or a line to send.
const STEP: Duration = Duration::from_millis(100);

#[derive(Default)]
pub(crate) struct Station {
    // keep-sorted start
    /// Bumped whenever a helper starts, so an older one ending leaves the
    /// newer one's state alone.
    generation: u64,
    /// Lines for the running helper; `None` while none runs.
    lines: Option<mpsc::Sender<String>>,
    /// The pairing dialog's advertisement while its [`Advert`] lives.
    pairing: Option<(Weak<()>, String)>,
    presence: bool,
    radio: Option<Radio>,
    // keep-sorted end
}

impl Global for Station {}

/// Held by the pairing dialog while its offer shows; dropping it stops
/// the pairing advertisement.
pub(crate) struct Advert {
    _token: Rc<()>,
}

/// What the running helper's loop does next.
enum Check {
    Stop,
    Run { stop_pairing: bool },
}

impl Station {
    pub(crate) fn install(cx: &mut App) {
        cx.set_global(Self::default());
    }

    /// Advertises this computer for pairing with the helper's `advertise`
    /// line until the returned [`Advert`] drops. `None` when the app has
    /// no station (tests, other modes).
    pub(crate) fn advertise_pairing(cx: &mut App, line: String) -> Option<Advert> {
        if !cx.has_global::<Self>() {
            return None;
        }
        let token = Rc::new(());
        let station = cx.global_mut::<Self>();
        station.pairing = Some((Rc::downgrade(&token), line.clone()));
        if let Some(lines) = &station.lines {
            let _ = lines.send(line);
        }
        Self::sync(cx);
        Some(Advert { _token: token })
    }

    /// Whether to advertise for paired phones' presence.
    pub(crate) fn set_presence(cx: &mut App, on: bool) {
        if !cx.has_global::<Self>() {
            return;
        }
        let station = cx.global_mut::<Self>();
        if station.presence == on {
            return;
        }
        station.presence = on;
        if let Some(lines) = &station.lines {
            let _ = lines.send(advertise_presence(on));
        }
        Self::sync(cx);
    }

    /// The computer's Bluetooth radio, while a helper runs.
    pub(crate) fn radio(cx: &App) -> Option<Radio> {
        cx.try_global::<Self>().and_then(|station| station.radio)
    }

    fn wanted(&self) -> bool {
        self.presence
            || self
                .pairing
                .as_ref()
                .is_some_and(|(token, _)| token.strong_count() > 0)
    }

    /// Starts a helper when one is wanted and none runs.
    fn sync(cx: &mut App) {
        let station = cx.global_mut::<Self>();
        if station.lines.is_some() || !station.wanted() {
            return;
        }
        let (lines, queued) = mpsc::channel();
        if let Some((_, line)) = &station.pairing {
            let _ = lines.send(line.clone());
        }
        if station.presence {
            let _ = lines.send(advertise_presence(true));
        }
        station.generation += 1;
        station.lines = Some(lines);
        let generation = station.generation;
        let client = Supervisor::client(cx);
        cx.spawn(async move |cx| {
            let started = cx
                .background_executor()
                .spawn(async move {
                    let exe = std::env::current_exe().ok();
                    let path = helper_path(exe.as_deref())?;
                    Helper::spawn(&path)
                        .inspect_err(|error| tracing::warn!(%error, "nearby helper is off"))
                        .ok()
                })
                .await;
            let ran = started.is_some();
            if let Some(helper) = started {
                run(&helper, &queued, client.as_ref(), generation, cx).await;
            }
            cx.update(|cx| Self::ended(cx, generation, ran));
        })
        .detach();
    }

    /// Drops a pairing advertisement whose dialog went, and stops the
    /// helper when nothing wants it any more.
    fn check(cx: &mut App, generation: u64) -> Check {
        if !cx.has_global::<Self>() {
            return Check::Stop;
        }
        let station = cx.global_mut::<Self>();
        if station.generation != generation {
            return Check::Stop;
        }
        let mut stop_pairing = false;
        if station
            .pairing
            .as_ref()
            .is_some_and(|(token, _)| token.strong_count() == 0)
        {
            station.pairing = None;
            stop_pairing = true;
        }
        if station.wanted() {
            Check::Run { stop_pairing }
        } else {
            Check::Stop
        }
    }

    /// Forgets a helper that stopped. One that ran starts again when
    /// something came to want it as it stopped; one that could not start
    /// waits for the next change rather than retrying in a loop.
    fn ended(cx: &mut App, generation: u64, ran: bool) {
        if !cx.has_global::<Self>() {
            return;
        }
        let station = cx.global_mut::<Self>();
        if station.generation != generation {
            return;
        }
        station.lines = None;
        station.radio = None;
        cx.refresh_windows();
        if ran {
            Self::sync(cx);
        }
    }

    fn set_radio(cx: &mut App, generation: u64, radio: Radio) {
        if !cx.has_global::<Self>() {
            return;
        }
        let station = cx.global_mut::<Self>();
        if station.generation == generation && station.radio != Some(radio) {
            station.radio = Some(radio);
            cx.refresh_windows();
        }
    }
}

/// The helper's loop: lines to it, its events, and presence's renewals,
/// until nothing wants it. Dropping the helper after it stops it.
async fn run(
    helper: &Helper,
    queued: &mpsc::Receiver<String>,
    client: Option<&Client>,
    generation: u64,
    cx: &mut gpui::AsyncApp,
) {
    let mut presence = PresenceRelay::default();
    loop {
        match cx.update(|cx| Station::check(cx, generation)) {
            Check::Stop => return,
            Check::Run { stop_pairing } => {
                if stop_pairing {
                    helper.send(STOP.to_owned());
                }
            }
        }
        while let Ok(line) = queued.try_recv() {
            helper.send(line);
        }
        match helper.events.try_recv() {
            Ok(Event::Radio(radio)) => cx.update(|cx| Station::set_radio(cx, generation, radio)),
            Ok(Event::Frame { central, data }) => {
                let reply = match (request(&data), client) {
                    (Ok(request), Some(client)) => answer(client.request(request).await),
                    _ => refusal(),
                };
                helper.send(frame(&central, &reply));
            }
            Ok(Event::PresenceFrame { central, data }) => {
                let reply = match (presence.receive(&central, &data, Instant::now()), client) {
                    (Received::Ask(request), Some(client)) => {
                        Some(presence.answered(&central, client.request(request).await))
                    }
                    (Received::Ask(_), None) => Some(refusal()),
                    (Received::Reply(reply), _) => Some(reply),
                    (Received::Nothing, _) => None,
                };
                if let Some(reply) = reply {
                    helper.send(presence_frame(&central, &reply));
                }
            }
            Ok(Event::Disconnected { central }) => presence.disconnected(&central),
            Ok(Event::Idle(seconds)) => presence.idle(seconds),
            Ok(Event::Failure(message)) => {
                tracing::warn!(%message, "the nearby helper reported a failure");
            }
            Err(TryRecvError::Empty) => {
                if let Some(client) = client {
                    for request in presence.due(Instant::now()) {
                        if let Err(error) = client.request(request).await {
                            tracing::debug!(%error, "presence was not sent");
                        }
                    }
                }
                cx.background_executor().timer(STEP).await;
            }
            Err(TryRecvError::Disconnected) => return,
        }
    }
}
