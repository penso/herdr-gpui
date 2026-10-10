//! Push notifications through an ntfy topic (<https://ntfy.sh>), so a phone
//! hears about prompts while the web app is closed. Sending happens on one
//! worker thread behind a bounded queue: a slow or unreachable ntfy server
//! drops notices instead of delaying hooks.
//!
//! Notices name the tool and the project directory but not commands or
//! messages unless `details` is on, because a public ntfy server sees them.

use crate::broker::{EventKind, Origin};
use std::{
    path::Path,
    sync::mpsc::{self, SyncSender},
    thread,
    time::Duration,
};

const QUEUE: usize = 32;
const MAX_BODY: usize = 300;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub title: String,
    pub body: String,
    /// ntfy priority, 1 (min) to 5 (max).
    pub priority: u8,
    pub tags: &'static str,
}

/// The notice for a feed event, if it is one a person should hear about.
pub fn notice_for(kind: &EventKind, origin: &Origin, details: bool) -> Option<Notice> {
    let project = origin
        .cwd
        .as_deref()
        .and_then(|cwd| Path::new(cwd).file_name())
        .map_or_else(
            || "an agent".to_owned(),
            |name| name.to_string_lossy().into_owned(),
        );
    let detail = |text: &str| {
        if details && !text.is_empty() {
            format!(": {}", crate::transcript::clip(text, MAX_BODY))
        } else {
            String::new()
        }
    };
    match kind {
        EventKind::PermissionRequested {
            tool_name, summary, ..
        } => Some(Notice {
            title: format!("Approval needed in {project}"),
            body: format!("{tool_name}{}", detail(summary)),
            priority: 4,
            tags: "lock",
        }),
        // Permission prompts already notified above; idle reminders repeat a stop.
        EventKind::Notification {
            notification_type, ..
        } if matches!(
            notification_type.as_deref(),
            Some("permission_prompt" | "idle_prompt")
        ) =>
        {
            None
        }
        EventKind::Notification { message, .. } => Some(Notice {
            title: format!("Input needed in {project}"),
            body: if details {
                crate::transcript::clip(message, MAX_BODY)
            } else {
                "Claude is waiting for you".to_owned()
            },
            priority: 4,
            tags: "speech_balloon",
        }),
        EventKind::Stopped {
            last_assistant_message,
        } => Some(Notice {
            title: format!("Finished in {project}"),
            body: format!(
                "Claude is done{}",
                detail(last_assistant_message.as_deref().unwrap_or_default())
            ),
            priority: 3,
            tags: "white_check_mark",
        }),
        _ => None,
    }
}

pub struct Notifier {
    sender: SyncSender<Notice>,
    pub details: bool,
}

impl Notifier {
    /// Starts the sender thread. `click` is the URL a tapped notice opens,
    /// normally the companion's web app.
    pub fn spawn(topic_url: String, click: Option<String>, details: bool) -> Self {
        let (sender, receiver) = mpsc::sync_channel::<Notice>(QUEUE);
        thread::spawn(move || {
            let agent: ureq::Agent = ureq::Agent::config_builder()
                .max_redirects(0)
                .timeout_global(Some(Duration::from_secs(15)))
                .build()
                .into();
            for notice in receiver {
                let mut request = agent
                    .post(&topic_url)
                    .header("Title", ascii_header(&notice.title))
                    .header("Priority", notice.priority.to_string())
                    .header("Tags", notice.tags);
                if let Some(click) = &click {
                    request = request.header("Click", click);
                }
                // A lost notice is acceptable; the web app still shows the
                // prompt, and the hook never waits on this thread.
                let _ = request.send(notice.body.as_str());
            }
        });
        Self { sender, details }
    }

    pub fn send(&self, notice: Notice) {
        let _ = self.sender.try_send(notice);
    }
}

/// HTTP header values must be visible ASCII; ntfy shows the body in full UTF-8.
fn ascii_header(text: &str) -> String {
    text.chars()
        .map(|ch| {
            if ch.is_ascii_graphic() || ch == ' ' {
                ch
            } else {
                '?'
            }
        })
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn origin() -> Origin {
        Origin {
            session_id: "s".into(),
            pane_id: None,
            cwd: Some("/home/u/api-server".into()),
        }
    }

    fn approval() -> EventKind {
        EventKind::PermissionRequested {
            request_id: 1,
            tool_name: "Bash".into(),
            summary: "rm -rf build".into(),
        }
    }

    #[test]
    fn approval_notices_hide_the_command_unless_asked() {
        let notice = notice_for(&approval(), &origin(), false).unwrap();
        assert_eq!(notice.title, "Approval needed in api-server");
        assert_eq!(notice.body, "Bash");
        let notice = notice_for(&approval(), &origin(), true).unwrap();
        assert_eq!(notice.body, "Bash: rm -rf build");
    }

    #[test]
    fn duplicate_and_noisy_events_are_silent() {
        let notification = |kind: &str| EventKind::Notification {
            notification_type: Some(kind.into()),
            message: "x".into(),
        };
        assert_eq!(
            notice_for(&notification("permission_prompt"), &origin(), true),
            None
        );
        assert_eq!(
            notice_for(&notification("idle_prompt"), &origin(), true),
            None
        );
        assert!(notice_for(&notification("elicitation_dialog"), &origin(), true).is_some());
        let tool = EventKind::ToolStarted {
            tool_name: "Bash".into(),
            summary: String::new(),
        };
        assert_eq!(notice_for(&tool, &origin(), true), None);
    }

    #[test]
    fn a_finished_turn_names_the_project() {
        let stop = EventKind::Stopped {
            last_assistant_message: Some("All tests pass.".into()),
        };
        let notice = notice_for(&stop, &Origin::default(), true).unwrap();
        assert_eq!(notice.title, "Finished in an agent");
        assert_eq!(notice.body, "Claude is done: All tests pass.");
    }

    #[test]
    fn titles_are_safe_header_values() {
        assert_eq!(ascii_header("Fini dans café\n"), "Fini dans caf??");
    }
}
