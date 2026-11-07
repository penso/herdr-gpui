//! The supervisor's part in answering phones about workspaces: while a phone
//! is connected it waits on the daemon for their questions, one long request
//! at a time, the next tick starting the next, and answers each on the
//! background executor. With no phone connected it asks nothing.

use super::*;
use crate::remote::changes::Sources;
use sheprd_remote_control::control::changes::Ask;

/// How long after a failed wait the daemon is asked again.
const RETRY: Duration = Duration::from_secs(5);

#[derive(Default)]
pub(crate) struct Changes {
    polling: bool,
    retry_at: Option<Instant>,
    sources: Arc<Sources>,
}

impl Supervisor {
    /// The workspaces phones may ask about, as a window last saw them.
    pub(super) fn set_change_sources(&mut self, sources: Sources) {
        self.changes.sources = Arc::new(sources);
    }

    /// Waits for phones' questions, unless that is underway or no phone
    /// could be asking.
    pub(super) fn poll_changes(cx: &mut App) {
        let executor = cx.background_executor().clone();
        let supervisor = cx.global_mut::<Self>();
        let phones = matches!(&supervisor.health, Health::Running(status) if status.connected > 0);
        let waiting = supervisor
            .changes
            .retry_at
            .is_some_and(|at| Instant::now() < at);
        if !phones || waiting || supervisor.changes.polling {
            return;
        }
        let Some(address) = supervisor.address.clone() else {
            return;
        };
        supervisor.changes.polling = true;
        let send = supervisor.send;
        let task = executor.spawn(async move { send(&address, &Request::ChangeAsks) });
        cx.spawn(async move |cx| {
            let answer = task.await;
            cx.update(|cx| Self::take_asks(cx, answer));
        })
        .detach();
    }

    fn take_asks(cx: &mut App, answer: Result<Response, control::Error>) {
        let executor = cx.background_executor().clone();
        let supervisor = cx.global_mut::<Self>();
        supervisor.changes.polling = false;
        let Ok(Response::ChangeAsks { asks }) = answer else {
            // A daemon that does not know the request answers at once.
            supervisor.changes.retry_at = Some(Instant::now() + RETRY);
            return;
        };
        supervisor.changes.retry_at = None;
        if let Some(address) = supervisor.address.clone() {
            let (send, sources) = (supervisor.send, supervisor.changes.sources.clone());
            for ask in asks {
                let (address, sources) = (address.clone(), sources.clone());
                executor
                    .spawn(async move { reply(&address, send, &sources, &ask) })
                    .detach();
            }
        }
    }
}

/// Works out one answer and hands it to the daemon. Blocking.
fn reply(address: &Path, send: Send, sources: &Sources, ask: &Ask) {
    let answer = sources.answer(ask);
    let id = ask.id;
    if let Err(error) = send(address, &Request::ChangeAnswer { id, answer }) {
        tracing::debug!(%error, "The daemon did not take an answer for a phone");
    }
}
