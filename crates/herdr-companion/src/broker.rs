//! Pending permission prompts and the bounded event feed, shared between the
//! hook connections that wait on a decision and the phone connections that
//! supply one. Nothing here touches a socket, so every rule is testable alone.

use crate::{Error, Result, transcript::tool_summary};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    path::PathBuf,
    sync::{Condvar, Mutex, MutexGuard, PoisonError},
    time::{Duration, Instant},
};

pub const ASK_USER_QUESTION: &str = "AskUserQuestion";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub max_pending: usize,
    pub max_events: usize,
    pub max_sessions: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_pending: 64,
            max_events: 1024,
            max_sessions: 256,
        }
    }
}

/// Where a hook call came from. `pane_id` is Herdr's `HERDR_PANE_ID`, which the
/// hook forwards as a header; it is what a reply prompt is later sent to.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Origin {
    pub session_id: String,
    pub pane_id: Option<String>,
    pub cwd: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PendingRequest {
    pub id: u64,
    #[serde(flatten)]
    pub origin: Origin,
    pub tool_name: String,
    pub tool_input: Value,
    pub tool_use_id: Option<String>,
}

/// A phone's answer to a pending request.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "behavior", rename_all = "snake_case")]
pub enum Decision {
    Allow {
        #[serde(default)]
        updated_input: Option<Value>,
        #[serde(default)]
        updated_permissions: Vec<String>,
    },
    /// Answers an `AskUserQuestion` call, keyed by question text. Values are a
    /// label, a list of labels for multi-select, or free text.
    Answer {
        answers: Map<String, Value>,
    },
    Deny {
        message: String,
    },
    /// Hand the prompt back to the terminal, where Claude Code shows it as usual.
    Terminal,
}

impl Decision {
    fn outcome(&self) -> Outcome {
        match self {
            Self::Allow { .. } => Outcome::Allowed,
            Self::Answer { .. } => Outcome::Answered,
            Self::Deny { .. } => Outcome::Denied,
            Self::Terminal => Outcome::Terminal,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Allowed,
    Answered,
    Denied,
    Terminal,
    TimedOut,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EventKind {
    PermissionRequested {
        request_id: u64,
        tool_name: String,
        summary: String,
    },
    PermissionResolved {
        request_id: u64,
        outcome: Outcome,
    },
    Notification {
        notification_type: Option<String>,
        message: String,
    },
    Stopped {
        last_assistant_message: Option<String>,
    },
    PromptSubmitted {
        prompt: String,
    },
    ToolStarted {
        tool_name: String,
        summary: String,
    },
    ToolFinished {
        tool_name: String,
        summary: String,
        failed: bool,
    },
    Hook {
        hook_event_name: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Event {
    pub seq: u64,
    #[serde(flatten)]
    pub origin: Origin,
    #[serde(flatten)]
    pub kind: EventKind,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EventPage {
    pub events: Vec<Event>,
    /// Pass back as `after` to continue.
    pub next: u64,
    /// Events after the cursor were dropped from the bounded feed; re-read
    /// `/v1/requests` rather than trusting the gap.
    pub lost: bool,
}

/// A Claude Code session the hooks have reported, newest activity first in
/// listings. The transcript path stays server-side; clients ask for the chat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Session {
    pub session_id: String,
    pub pane_id: Option<String>,
    pub cwd: Option<String>,
    #[serde(skip)]
    pub transcript_path: Option<PathBuf>,
    pub has_transcript: bool,
    /// The feed sequence of its latest hook call; orders sessions by recency.
    pub last_seq: u64,
}

struct Slot {
    request: PendingRequest,
    decision: Option<Decision>,
}

#[derive(Default)]
struct State {
    next_request: u64,
    last_seq: u64,
    pending: BTreeMap<u64, Slot>,
    events: VecDeque<Event>,
    sessions: HashMap<String, Session>,
}

impl State {
    fn record(&mut self, limit: usize, origin: Origin, kind: EventKind) {
        self.last_seq += 1;
        if self.events.len() == limit {
            self.events.pop_front();
        }
        self.events.push_back(Event {
            seq: self.last_seq,
            origin,
            kind,
        });
    }
}

#[derive(Default)]
pub struct Broker {
    limits: Limits,
    state: Mutex<State>,
    changed: Condvar,
}

impl Broker {
    pub fn new(limits: Limits) -> Self {
        Self {
            limits,
            ..Self::default()
        }
    }

    /// A panicking connection thread must not take every other client down,
    /// and the state stays consistent because no update spans an unwind point.
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn open(
        &self,
        origin: Origin,
        tool_name: String,
        tool_input: Value,
        tool_use_id: Option<String>,
    ) -> Result<u64> {
        let mut state = self.lock();
        if state.pending.len() >= self.limits.max_pending {
            return Err(Error::PendingFull);
        }
        state.next_request += 1;
        let id = state.next_request;
        let kind = EventKind::PermissionRequested {
            request_id: id,
            tool_name: tool_name.clone(),
            summary: tool_summary(&tool_name, &tool_input),
        };
        state.record(self.limits.max_events, origin.clone(), kind);
        let request = PendingRequest {
            id,
            origin,
            tool_name,
            tool_input,
            tool_use_id,
        };
        state.pending.insert(
            id,
            Slot {
                request,
                decision: None,
            },
        );
        drop(state);
        self.changed.notify_all();
        Ok(id)
    }

    /// Blocks the hook connection until a decision arrives or `timeout`
    /// passes. Either way the request leaves the pending set, so a late phone
    /// answer is refused rather than silently applied to nothing.
    pub fn await_decision(&self, id: u64, timeout: Duration) -> Option<(PendingRequest, Decision)> {
        let deadline = Instant::now() + timeout;
        let mut state = self.lock();
        loop {
            if state.pending.get(&id)?.decision.is_some() {
                break;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            state = self
                .changed
                .wait_timeout(state, remaining)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
        let slot = state.pending.remove(&id)?;
        let outcome = slot
            .decision
            .as_ref()
            .map_or(Outcome::TimedOut, Decision::outcome);
        let kind = EventKind::PermissionResolved {
            request_id: id,
            outcome,
        };
        state.record(self.limits.max_events, slot.request.origin.clone(), kind);
        drop(state);
        self.changed.notify_all();
        slot.decision.map(|decision| (slot.request, decision))
    }

    pub fn decide(&self, id: u64, decision: Decision) -> Result<()> {
        match &decision {
            Decision::Deny { message } if message.trim().is_empty() => {
                return Err(Error::EmptyDenyMessage);
            }
            _ => {}
        }
        let mut state = self.lock();
        let slot = state
            .pending
            .get_mut(&id)
            .filter(|slot| slot.decision.is_none())
            .ok_or(Error::UnknownRequest(id))?;
        if matches!(decision, Decision::Answer { .. })
            && slot.request.tool_name != ASK_USER_QUESTION
        {
            return Err(Error::NotAQuestion(id));
        }
        slot.decision = Some(decision);
        drop(state);
        self.changed.notify_all();
        Ok(())
    }

    /// Requests still waiting for a decision, oldest first.
    pub fn pending(&self) -> Vec<PendingRequest> {
        self.lock()
            .pending
            .values()
            .filter(|slot| slot.decision.is_none())
            .map(|slot| slot.request.clone())
            .collect()
    }

    /// Remembers which pane, directory, and transcript a session belongs to.
    /// Fields a hook omits keep their earlier values.
    pub fn touch_session(&self, origin: &Origin, transcript_path: Option<PathBuf>) {
        if origin.session_id.is_empty() {
            return;
        }
        let mut state = self.lock();
        let last_seq = state.last_seq;
        let session = state
            .sessions
            .entry(origin.session_id.clone())
            .or_insert_with(|| Session {
                session_id: origin.session_id.clone(),
                pane_id: None,
                cwd: None,
                transcript_path: None,
                has_transcript: false,
                last_seq,
            });
        session.last_seq = last_seq;
        if origin.pane_id.is_some() {
            session.pane_id.clone_from(&origin.pane_id);
        }
        if origin.cwd.is_some() {
            session.cwd.clone_from(&origin.cwd);
        }
        if transcript_path.is_some() {
            session.transcript_path = transcript_path;
            session.has_transcript = true;
        }
        if state.sessions.len() > self.limits.max_sessions {
            let stalest = state
                .sessions
                .values()
                .min_by_key(|session| session.last_seq)
                .map(|session| session.session_id.clone());
            if let Some(stalest) = stalest {
                state.sessions.remove(&stalest);
            }
        }
    }

    /// Known sessions, most recently active first.
    pub fn sessions(&self) -> Vec<Session> {
        let mut sessions: Vec<_> = self.lock().sessions.values().cloned().collect();
        sessions.sort_by_key(|session| std::cmp::Reverse(session.last_seq));
        sessions
    }

    pub fn session(&self, session_id: &str) -> Option<Session> {
        self.lock().sessions.get(session_id).cloned()
    }

    pub fn record(&self, origin: Origin, kind: EventKind) {
        self.lock().record(self.limits.max_events, origin, kind);
        self.changed.notify_all();
    }

    /// Events after `after`, waiting up to `wait` for one to arrive.
    pub fn events_after(&self, after: u64, wait: Duration) -> EventPage {
        let deadline = Instant::now() + wait;
        let mut state = self.lock();
        while state.last_seq <= after {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            state = self
                .changed
                .wait_timeout(state, remaining)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
        let lost = state
            .events
            .front()
            .is_some_and(|first| first.seq > after.saturating_add(1));
        EventPage {
            events: state
                .events
                .iter()
                .filter(|event| event.seq > after)
                .cloned()
                .collect(),
            next: state.last_seq.max(after),
            lost,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::{sync::Arc, thread};

    fn origin() -> Origin {
        Origin {
            session_id: "s".into(),
            pane_id: Some("p_1".into()),
            cwd: None,
        }
    }

    fn open(broker: &Broker, tool: &str) -> u64 {
        broker.open(origin(), tool.into(), json!({}), None).unwrap()
    }

    #[test]
    fn a_decision_wakes_the_waiting_hook() {
        let broker = Arc::new(Broker::default());
        let id = open(&broker, "Bash");
        let waiter = {
            let broker = Arc::clone(&broker);
            thread::spawn(move || broker.await_decision(id, Duration::from_secs(30)))
        };
        // Wait until the request is visible, as a phone would see it.
        while broker.pending().is_empty() {
            thread::yield_now();
        }
        broker
            .decide(
                id,
                Decision::Deny {
                    message: "no".into(),
                },
            )
            .unwrap();
        let (request, decision) = waiter.join().unwrap().unwrap();
        assert_eq!(request.id, id);
        assert_eq!(
            decision,
            Decision::Deny {
                message: "no".into()
            }
        );
        assert!(broker.pending().is_empty());
    }

    #[test]
    fn a_timeout_removes_the_request_and_refuses_a_late_answer() {
        let broker = Broker::default();
        let id = open(&broker, "Bash");
        assert_eq!(broker.await_decision(id, Duration::ZERO), None);
        assert!(matches!(
            broker.decide(id, Decision::Terminal),
            Err(Error::UnknownRequest(found)) if found == id
        ));
        let page = broker.events_after(0, Duration::ZERO);
        assert_eq!(
            page.events.last().map(|event| &event.kind),
            Some(&EventKind::PermissionResolved {
                request_id: id,
                outcome: Outcome::TimedOut
            })
        );
    }

    #[test]
    fn a_request_takes_only_one_decision() {
        let broker = Broker::default();
        let id = open(&broker, "Bash");
        broker.decide(id, Decision::Terminal).unwrap();
        assert!(matches!(
            broker.decide(id, Decision::Terminal),
            Err(Error::UnknownRequest(_))
        ));
        assert!(
            broker.pending().is_empty(),
            "a decided request is no longer pending"
        );
    }

    #[test]
    fn answers_apply_only_to_questions_and_denials_need_a_reason() {
        let broker = Broker::default();
        let bash = open(&broker, "Bash");
        let answer = Decision::Answer {
            answers: Map::new(),
        };
        assert!(matches!(
            broker.decide(bash, answer.clone()),
            Err(Error::NotAQuestion(found)) if found == bash
        ));
        assert!(matches!(
            broker.decide(
                bash,
                Decision::Deny {
                    message: " ".into()
                }
            ),
            Err(Error::EmptyDenyMessage)
        ));
        let question = open(&broker, ASK_USER_QUESTION);
        broker.decide(question, answer).unwrap();
    }

    #[test]
    fn pending_requests_are_bounded() {
        let broker = Broker::new(Limits {
            max_pending: 1,
            max_events: 8,
            max_sessions: 8,
        });
        open(&broker, "Bash");
        assert!(matches!(
            broker.open(origin(), "Bash".into(), json!({}), None),
            Err(Error::PendingFull)
        ));
    }

    #[test]
    fn the_event_feed_is_bounded_and_reports_a_gap() {
        let broker = Broker::new(Limits {
            max_pending: 1,
            max_events: 2,
            max_sessions: 8,
        });
        for prompt in ["a", "b", "c"] {
            broker.record(
                origin(),
                EventKind::PromptSubmitted {
                    prompt: prompt.into(),
                },
            );
        }
        let page = broker.events_after(0, Duration::ZERO);
        assert!(page.lost);
        assert_eq!(page.next, 3);
        assert_eq!(
            page.events
                .iter()
                .map(|event| event.seq)
                .collect::<Vec<_>>(),
            [2, 3]
        );
        let page = broker.events_after(1, Duration::ZERO);
        assert!(!page.lost, "seq 2 is still retained");
        let page = broker.events_after(3, Duration::ZERO);
        assert!(page.events.is_empty() && !page.lost);
        assert_eq!(page.next, 3);
    }

    #[test]
    fn a_long_poll_returns_as_soon_as_an_event_arrives() {
        let broker = Arc::new(Broker::default());
        let poller = {
            let broker = Arc::clone(&broker);
            thread::spawn(move || broker.events_after(0, Duration::from_secs(30)))
        };
        broker.record(
            origin(),
            EventKind::Stopped {
                last_assistant_message: None,
            },
        );
        let page = poller.join().unwrap();
        assert_eq!(page.next, 1);
    }

    #[test]
    fn sessions_remember_their_pane_and_transcript_and_stay_bounded() {
        let broker = Broker::new(Limits {
            max_pending: 1,
            max_events: 8,
            max_sessions: 2,
        });
        broker.touch_session(&origin(), Some("/t/s.jsonl".into()));
        // A later hook without a pane or transcript keeps what was learned.
        broker.record(
            origin(),
            EventKind::Stopped {
                last_assistant_message: None,
            },
        );
        broker.touch_session(
            &Origin {
                session_id: "s".into(),
                pane_id: None,
                cwd: Some("/w".into()),
            },
            None,
        );
        let session = broker.session("s").unwrap();
        assert_eq!(session.pane_id.as_deref(), Some("p_1"));
        assert_eq!(session.cwd.as_deref(), Some("/w"));
        assert_eq!(session.transcript_path, Some(PathBuf::from("/t/s.jsonl")));
        assert!(session.has_transcript);

        for id in ["a", "b"] {
            broker.record(
                origin(),
                EventKind::Stopped {
                    last_assistant_message: None,
                },
            );
            broker.touch_session(
                &Origin {
                    session_id: id.into(),
                    ..Origin::default()
                },
                None,
            );
        }
        let ids: Vec<_> = broker
            .sessions()
            .into_iter()
            .map(|session| session.session_id)
            .collect();
        assert_eq!(ids, ["b", "a"], "the stalest session was evicted");
        broker.touch_session(&Origin::default(), None);
        assert_eq!(
            broker.sessions().len(),
            2,
            "hooks without a session id are ignored"
        );
    }

    #[test]
    fn decisions_deserialize_from_the_phone_shape() {
        let allow: Decision = serde_json::from_value(json!({"behavior": "allow"})).unwrap();
        assert_eq!(
            allow,
            Decision::Allow {
                updated_input: None,
                updated_permissions: Vec::new()
            }
        );
        let answer: Decision = serde_json::from_value(
            json!({"behavior": "answer", "answers": {"Which?": ["A", "B"]}}),
        )
        .unwrap();
        assert!(matches!(answer, Decision::Answer { answers } if answers.len() == 1));
        assert!(serde_json::from_value::<Decision>(json!({"behavior": "maybe"})).is_err());
    }
}
