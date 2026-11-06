//! The control socket's messages: one JSON request line, one JSON response
//! line, then the connection closes. Browser requests keep their original
//! flat form; every other family sends `{"method", "caller", "op"}` with a
//! family-specific `op`, and is answered with a family-specific `reply`.
pub mod app;
pub mod chat;
pub mod cloud;
pub mod events;
pub mod hosts;
pub mod mods;
pub mod remote;
pub mod settings;
pub mod tabs;
pub mod work;

use serde::{Deserialize, Serialize};

/// Where `browser open` puts a new page.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Placement {
    /// A tab of the group in use, which takes the window's view.
    #[default]
    Tab,
    /// A group beside the caller's terminal, so both stay in view.
    Beside,
}

/// Raised whenever a request or reply changes shape, so `status` can tell
/// an outdated app from a refused request. 3: `browser.open` placement.
/// 4: `link`. 5: the `tab` family's `zoom`. 6: the `cloud` family. 7: `chat`.
/// 8: `host` `automation_approve`. 9: `remote`. 10: `settings` `language`.
/// 11: `mod`.
pub const PROTOCOL_VERSION: u32 = 11;

/// Older apps and clients know only tabs, so a tab is left off the wire.
fn is_tab(placement: &Placement) -> bool {
    *placement == Placement::Tab
}

/// Bounds a request and a response alike. A response can carry a batch of
/// notes, each with a bounded snippet of the page.
pub const MAX_MESSAGE: usize = 256 * 1024;
/// The longest `browser feedback --wait`, in seconds.
pub const MAX_WAIT_SECONDS: u64 = 600;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "method", deny_unknown_fields)]
pub enum Request {
    #[serde(rename = "browser.open")]
    Open(BrowserOpen),
    #[serde(rename = "browser.reload")]
    Reload(Caller),
    #[serde(rename = "browser.feedback")]
    Feedback(FeedbackRequest),
    #[serde(rename = "notes.send")]
    Notes(NotesRequest),
    #[serde(rename = "status")]
    Status(Caller),
    #[serde(rename = "app")]
    App(Call<app::AppOp>),
    #[serde(rename = "window")]
    Window(Call<app::WindowOp>),
    #[serde(rename = "ui")]
    Ui(Call<app::UiOp>),
    #[serde(rename = "command")]
    Command(Call<app::CommandOp>),
    #[serde(rename = "tab")]
    Tab(Call<tabs::TabOp>),
    #[serde(rename = "work")]
    Work(Call<work::WorkOp>),
    #[serde(rename = "host")]
    Host(Call<hosts::HostOp>),
    #[serde(rename = "settings")]
    Settings(Call<settings::SettingsOp>),
    #[serde(rename = "events")]
    Events(Call<events::EventsOp>),
    #[serde(rename = "link")]
    Link(Call<LinkOp>),
    #[serde(rename = "chat")]
    Chat(Call<chat::ChatOp>),
    #[serde(rename = "cloud")]
    Cloud(Call<cloud::CloudOp>),
    #[serde(rename = "remote")]
    Remote(Call<remote::RemoteOp>),
    #[serde(rename = "mod")]
    Mod(Call<mods::ModOp>),
}

/// A `herdr-gpui://` link the shell started a second copy of the app with.
/// The app parses it again: any local process can send one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LinkOp {
    pub url: String,
}

/// A request of a family other than the browser's: who asks, and what.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Call<T> {
    pub caller: Caller,
    pub op: T,
}

impl Request {
    /// How long the app may take to answer, beyond the usual bound: some
    /// requests wait for the user, a page, or an event on purpose.
    pub fn wait_seconds(&self) -> u64 {
        let wait = match self {
            Self::Feedback(request) => request.wait_seconds,
            Self::Events(call) => call.op.wait_seconds(),
            Self::Tab(call) => call.op.wait_seconds(),
            Self::Host(call) => call.op.wait_seconds(),
            Self::Settings(call) => call.op.wait_seconds(),
            Self::Cloud(call) => call.op.wait_seconds(),
            Self::Remote(call) => call.op.wait_seconds(),
            _ => 0,
        };
        wait.min(MAX_WAIT_SECONDS)
    }
}

/// What a tab shows: a web address, or a local file by absolute path, which
/// the caller resolved on its side.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Page {
    Url(String),
    File(String),
}

/// Who is asking: the Herdr pane, workspace, and daemon the caller runs in.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Caller {
    /// The caller's `HERDR_PANE_ID`, which notes on its pages go back to.
    #[serde(default)]
    pub pane_id: Option<String>,
    /// The Herdr workspace to open the tab in, normally the caller's own
    /// `HERDR_WORKSPACE_ID`. Absent, the tab opens in the workspace the
    /// frontmost window shows.
    #[serde(default)]
    pub workspace_id: Option<String>,
    /// The client socket of the caller's daemon, which tells two local
    /// sessions' identical workspace IDs apart.
    #[serde(default)]
    pub daemon_socket: Option<String>,
    /// The window to act in, by the ID `window list` reports.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrowserOpen {
    pub page: Page,
    pub caller: Caller,
    /// Whether the window switches to the tab.
    pub focus: bool,
    #[serde(default, skip_serializing_if = "is_tab")]
    pub placement: Placement,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeedbackRequest {
    pub pane_id: String,
    /// Absent in older clients; never treated as a wildcard for a daemon.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub daemon_socket: Option<String>,
    /// How long to wait for notes when none are waiting, at most
    /// `MAX_WAIT_SECONDS`.
    pub wait_seconds: u64,
}

/// Notes the user wrote outside a window, such as in a UI mockup, for the
/// agent in the caller's pane. They go the way page notes do.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NotesRequest {
    pub caller: Caller,
    pub text: String,
}

/// Where sent notes went.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotesTo {
    /// To the agent: a window showing its pane types them in once it is
    /// idle, or a waiting `browser feedback --wait` takes them.
    Agent,
    /// Kept for `browser feedback`, since no window can type into an agent there.
    Kept,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpenedIn {
    /// A browser tab inside the window.
    Tab,
    /// The system browser, on a platform whose build cannot show pages.
    SystemBrowser,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidRequest,
    InvalidUrl,
    NoWindow,
    WorkspaceNotFound,
    TabLimit,
    Busy,
    Timeout,
    Unsupported,
    /// The window, tab, host, checkpoint, or other thing named is not there.
    NotFound,
    /// The app tried and the operation failed, for example a Git error.
    Failed,
    /// A dialog or menu holds the window, so it takes no commands now.
    WindowBusy,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Response {
    Opened {
        opened_in: OpenedIn,
        workspace_id: Option<String>,
        /// Where the page went: beside the terminal only when asked and
        /// the window could split.
        #[serde(default, skip_serializing_if = "is_tab")]
        placement: Placement,
    },
    Reloaded {
        tabs: usize,
    },
    /// Notes the user sent to the caller, or `None` when there are none.
    Feedback {
        text: Option<String>,
    },
    NotesSent {
        to: NotesTo,
    },
    Status(app::Status),
    /// Done; the message says what happened, for a person.
    Done {
        message: String,
    },
    App {
        reply: app::Reply,
    },
    Tab {
        reply: tabs::Reply,
    },
    Work {
        reply: work::Reply,
    },
    Host {
        reply: hosts::Reply,
    },
    Settings {
        reply: settings::Reply,
    },
    Events {
        reply: events::Reply,
    },
    Chat {
        reply: chat::Reply,
    },
    Cloud {
        reply: cloud::Reply,
    },
    Remote {
        reply: remote::Reply,
    },
    Mod {
        reply: mods::Reply,
    },
    Error {
        code: ErrorCode,
        message: String,
    },
}

impl Response {
    pub fn error(code: ErrorCode, message: impl Into<String>) -> Self {
        Self::Error {
            code,
            message: message.into(),
        }
    }

    pub fn done(message: impl Into<String>) -> Self {
        Self::Done {
            message: message.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn requests_have_a_stable_wire_form() {
        let request = Request::Open(BrowserOpen {
            page: Page::Url("http://localhost:3000/".into()),
            caller: Caller {
                pane_id: Some("w_1:p1".into()),
                workspace_id: Some("w_1".into()),
                daemon_socket: None,
                window: None,
            },
            focus: true,
            placement: Placement::Tab,
        });
        let text = serde_json::to_string(&request).unwrap();
        assert_eq!(
            text,
            r#"{"method":"browser.open","page":{"url":"http://localhost:3000/"},"caller":{"pane_id":"w_1:p1","workspace_id":"w_1","daemon_socket":null},"focus":true}"#
        );
        assert_eq!(serde_json::from_str::<Request>(&text).unwrap(), request);
        let beside: Request = serde_json::from_str(
            r#"{"method":"browser.open","page":{"file":"/srv/a.html"},"caller":{},"focus":true,"placement":"beside"}"#,
        )
        .unwrap();
        assert!(matches!(
            beside,
            Request::Open(BrowserOpen {
                placement: Placement::Beside,
                ..
            })
        ));
        let minimal: Request = serde_json::from_str(
            r#"{"method":"browser.open","page":{"file":"/srv/a.html"},"caller":{},"focus":false}"#,
        )
        .unwrap();
        assert!(matches!(
            minimal,
            Request::Open(BrowserOpen {
                page: Page::File(_),
                caller: Caller { pane_id: None, .. },
                ..
            })
        ));
        let feedback: Request = serde_json::from_str(
            r#"{"method":"browser.feedback","pane_id":"w_1:p1","wait_seconds":30}"#,
        )
        .unwrap();
        assert_eq!(
            feedback,
            Request::Feedback(FeedbackRequest {
                daemon_socket: None,
                pane_id: "w_1:p1".into(),
                wait_seconds: 30
            })
        );
        let notes: Request = serde_json::from_str(
            r#"{"method":"notes.send","caller":{"pane_id":"w_1:p1"},"text":"Picked: B"}"#,
        )
        .unwrap();
        assert_eq!(
            notes,
            Request::Notes(NotesRequest {
                caller: Caller {
                    pane_id: Some("w_1:p1".into()),
                    ..Caller::default()
                },
                text: "Picked: B".into(),
            })
        );
        let reload: Request =
            serde_json::from_str(r#"{"method":"browser.reload","pane_id":"w_1:p1"}"#).unwrap();
        assert!(matches!(
            reload,
            Request::Reload(Caller {
                pane_id: Some(_),
                ..
            })
        ));
    }

    #[test]
    fn unknown_methods_and_fields_are_rejected() {
        for invalid in [
            r#"{"method":"browser.eval","page":{"url":"https://a.test"},"caller":{},"focus":true}"#,
            r#"{"method":"browser.open","page":{"url":"https://a.test"},"caller":{},"focus":true,"extra":1}"#,
            r#"{"method":"browser.open","page":{"script":"x"},"caller":{},"focus":true}"#,
            r#"{"method":"browser.open","page":{"url":"https://a.test"},"caller":{},"focus":true,"placement":"popup"}"#,
            r#"{"method":"browser.open","page":{"url":"https://a.test"},"caller":{"shell":"x"},"focus":true}"#,
            r#"{"method":"browser.open","focus":true}"#,
            r#"{"method":"browser.feedback","wait_seconds":1}"#,
            r#"{"method":"notes.send","caller":{}}"#,
            r#"{"method":"notes.send","caller":{},"text":"x","paste":true}"#,
            r#"["browser.open"]"#,
        ] {
            assert!(
                serde_json::from_str::<Request>(invalid).is_err(),
                "{invalid}"
            );
        }
    }

    #[test]
    fn responses_round_trip() {
        for response in [
            Response::Opened {
                opened_in: OpenedIn::Tab,
                workspace_id: Some("w_1".into()),
                placement: Placement::Tab,
            },
            Response::Opened {
                opened_in: OpenedIn::Tab,
                workspace_id: Some("w_1".into()),
                placement: Placement::Beside,
            },
            Response::error(ErrorCode::WorkspaceNotFound, "No window shows w_9"),
            Response::Reloaded { tabs: 2 },
            Response::Feedback {
                text: Some("1. Note".into()),
            },
            Response::Feedback { text: None },
            Response::NotesSent { to: NotesTo::Agent },
            Response::NotesSent { to: NotesTo::Kept },
        ] {
            let text = serde_json::to_string(&response).unwrap();
            assert_eq!(serde_json::from_str::<Response>(&text).unwrap(), response);
        }
        assert_eq!(
            serde_json::to_string(&Response::error(ErrorCode::Busy, "x")).unwrap(),
            r#"{"status":"error","code":"busy","message":"x"}"#
        );
    }
}
