//! A companion bridge for agents running inside Herdr. Claude Code hooks post
//! here; a phone, through the built-in web app or any HTTP client, lists the
//! agents and what is waiting, answers permission prompts and `AskUserQuestion`
//! forms, reads each session as a chat, follows a bounded event feed, and sends
//! replies, interrupts, and new agents through Herdr's JSON API. Herdr keeps
//! owning every terminal; when no answer arrives in time the prompt falls back
//! to that terminal.

mod broker;
mod cli;
mod error;
mod herdr_api;
mod hook;
mod http;
mod notify;
mod pairing;
mod server;
mod transcript;
mod web;

pub use broker::{
    Broker, Decision, Event, EventKind, EventPage, Limits, Origin, Outcome, PendingRequest, Session,
};
pub use cli::{Command, ServeOptions, hooks_settings, parse_args, usage};
pub use error::{Error, HttpError, Result};
pub use herdr_api::{Agent, AgentStatus, Herdr, Placement, StartAgent, Workspace, default_socket};
pub use notify::{Notice, Notifier, notice_for};
pub use pairing::{PhoneUrl, pairing_url, phone_urls, render_qr, route_probe};
pub use server::{Companion, Config, serve};
pub use transcript::Entry;
