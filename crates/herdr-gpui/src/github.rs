//! GitHub sign-in as the app uses it. `herdr-github` owns the session, its
//! requests, and the credential store; the app keeps each account's avatar
//! (`avatars::ProfileAvatars`) and draws the menus.

#[cfg(test)]
pub(crate) use herdr_github::fake_graphql;
pub(crate) use herdr_github::{
    Access, Account, Auth, Installation, Note, Profile, Source, Store, Tokens, VERIFY_URL, forget,
    graphql, rest, select_store, store_note,
};
