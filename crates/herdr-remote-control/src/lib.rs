//! Remote control: a daemon that serves this machine's `herdr-acp` chats to
//! paired phones over HTTPS, the control protocol Herdr UI uses to run it,
//! and Herdr UI's side without the server ([`app`]).
#![doc = include_str!("../README.md")]

#[cfg(feature = "server")]
mod apns;
pub mod app;
#[cfg(feature = "server")]
mod auth;
pub mod control;
mod error;
#[cfg(feature = "server")]
mod hub;
#[cfg(feature = "server")]
mod push;
#[cfg(feature = "server")]
pub mod server;
#[cfg(feature = "server")]
mod tls;
#[cfg(feature = "server")]
mod web;

pub use error::{Error, Result};
