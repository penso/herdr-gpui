//! When a followed terminal's screen is read next. Herdr's API has no
//! output events, so screens are read on a timer; a phone that types
//! raises [`Typed`] for the pane, and its followers read at once and again
//! shortly after, so the echo comes back without waiting out the timer.

use std::{collections::VecDeque, time::Duration};
use tokio::{
    sync::broadcast::{self, error::RecvError},
    time::Instant,
};

/// How often a terminal's screen is read while a phone follows it.
const SCREEN_INTERVAL: Duration = Duration::from_millis(300);
/// Reads after input reaches a pane, from when it did: at once for a shell
/// echoing in place, then twice more for a program slower to redraw.
const ECHO_READS: [Duration; 3] = [
    Duration::ZERO,
    Duration::from_millis(50),
    Duration::from_millis(150),
];
/// Typing signals held for a slow follower; one that falls behind reads
/// as if its pane was typed into.
const TYPED_QUEUE: usize = 64;

/// The panes phones type into, as they do.
pub(crate) struct Typed(broadcast::Sender<String>);

impl Default for Typed {
    fn default() -> Self {
        Self(broadcast::channel(TYPED_QUEUE).0)
    }
}

impl Typed {
    /// Input reached `pane`.
    pub(crate) fn typed(&self, pane: &str) {
        // No follower is not a failure: nobody watches that pane.
        let _ = self.0.send(pane.to_owned());
    }

    /// The read timing for one follower of `pane`.
    pub(crate) fn pace(&self, pane: &str) -> Pace {
        Pace {
            echoes: VecDeque::new(),
            pane: pane.to_owned(),
            typed: Some(self.0.subscribe()),
        }
    }
}

/// One follower's read timing: every [`SCREEN_INTERVAL`], and the
/// [`ECHO_READS`] after its pane is typed into.
pub(crate) struct Pace {
    /// Reads due for an echo, earliest first.
    echoes: VecDeque<Instant>,
    pane: String,
    /// `None` once the sender is gone, which leaves only the timer.
    typed: Option<broadcast::Receiver<String>>,
}

impl Pace {
    /// Waits until the screen is due to be read again.
    pub(crate) async fn wait(&mut self) {
        let regular = Instant::now() + SCREEN_INTERVAL;
        loop {
            let due = self
                .echoes
                .front()
                .map_or(regular, |echo| regular.min(*echo));
            let typed = async {
                match &mut self.typed {
                    Some(typed) => typed.recv().await,
                    None => std::future::pending().await,
                }
            };
            tokio::select! {
                () = tokio::time::sleep_until(due) => break,
                signal = typed => match signal {
                    Ok(pane) if pane == self.pane => self.typed_now(),
                    Ok(_) => {}
                    Err(RecvError::Lagged(_)) => self.typed_now(),
                    Err(RecvError::Closed) => self.typed = None,
                },
            }
        }
        // This read covers every echo due by now.
        let now = Instant::now();
        while self.echoes.front().is_some_and(|echo| *echo <= now) {
            self.echoes.pop_front();
        }
    }

    fn typed_now(&mut self) {
        let now = Instant::now();
        self.echoes = ECHO_READS.iter().map(|delay| now + *delay).collect();
    }
}
