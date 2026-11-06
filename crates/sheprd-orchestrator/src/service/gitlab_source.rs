//! Reading a GitLab source with the instance's token, looked up once and
//! looked up again when GitLab refuses it: a connected account's token is
//! renewed by the lookup, by this worker or by the account menu.

use super::{Error, Result, SourceKey, Worker, gitlab};
use crate::Checkpoint;
use std::sync::atomic::Ordering;

impl Worker {
    /// Lists GitLab source `key`, continuing from `previous`.
    pub(super) fn gitlab_listing(
        &mut self,
        key: &SourceKey,
        previous: Option<&Checkpoint>,
    ) -> Result<gitlab::Listing> {
        let scheme = gitlab::Scheme::of_source(self.snapshot.repo.as_ref(), key);
        let api = gitlab::Api::resolve(&key.host, scheme);
        let looked_up = self
            .gitlab_token
            .as_ref()
            .is_some_and(|(instance, _)| *instance == api.instance);
        if !looked_up {
            self.look_up_gitlab_token(&api.instance);
        }
        match self.read_gitlab(key, &api, previous) {
            // The token kept since may have expired, or been renewed or
            // replaced elsewhere: look again before asking to sign in.
            Err(Error::Gitlab(gitlab::Error::Authentication)) if looked_up => {
                self.look_up_gitlab_token(&api.instance);
                self.read_gitlab(key, &api, previous)
            }
            result => result,
        }
    }

    /// Looks up `instance`'s token, renewing a connected account's when due.
    fn look_up_gitlab_token(&mut self, instance: &str) {
        // A remote checkout's .envrc is that machine's, not run here.
        let checkout = (!self.request.target.is_remote())
            .then(|| std::path::Path::new(&self.request.checkout));
        let cancelled = &self.cancelled;
        let token = gitlab::token(instance, self.request.gitlab_store, checkout, &|| {
            cancelled.load(Ordering::Relaxed)
        });
        self.gitlab_token = Some((instance.to_owned(), token));
    }

    fn read_gitlab(
        &mut self,
        key: &SourceKey,
        api: &gitlab::Api,
        previous: Option<&Checkpoint>,
    ) -> Result<gitlab::Listing> {
        let token = self
            .gitlab_token
            .as_ref()
            .and_then(|(_, token)| token.as_ref());
        let cancelled = &self.cancelled;
        gitlab::sync(
            (key, api),
            token.map(|found| &found.token),
            previous,
            &|| cancelled.load(Ordering::Relaxed),
            &mut self.cooldown,
        )
        .map_err(|error| match (error, token) {
            (Error::Gitlab(error), Some(found)) => Error::Gitlab(error.from_source(found.source)),
            (error, _) => error,
        })
    }
}
