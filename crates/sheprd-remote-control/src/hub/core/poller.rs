//! What only the worker reads: the desktop's document and the saved tabs,
//! kept between polls so an unchanged one is not parsed or read again.

use super::{Result, projection, spaces};
use crate::hub::Layout;
use sheprd_core_client::{
    Error,
    wire::{Doc, Terminal},
};
use std::{path::Path, time::SystemTime};

#[derive(Default)]
pub(super) struct Poller {
    /// The document's version and its spaces, `None` when it has none.
    doc: Option<(u64, Option<spaces::Spaces>)>,
    names: spaces::Names,
    saved: Option<(SystemTime, Option<projection::Saved>)>,
}

impl Poller {
    /// Takes the answer to reading the desktop's document. Only a core
    /// that says there is none clears the spaces: a slow answer says
    /// nothing about the document, and dropping them would make every
    /// space vanish from phones until the next poll.
    pub(super) fn read_spaces(&mut self, answer: sheprd_core_client::Result<Doc>) -> Result<()> {
        match answer {
            Ok(doc) => {
                if self.doc.as_ref().is_none_or(|(at, _)| *at != doc.version) {
                    self.doc = Some((doc.version, spaces::Spaces::read(doc.value)));
                }
            }
            Err(Error::Refused { .. }) => self.doc = None,
            Err(Error::Lost) => return Err(Error::Lost.into()),
            Err(error) => tracing::debug!(%error, "kept the desktop's spaces"),
        }
        Ok(())
    }

    pub(super) fn project(
        &mut self,
        boot: &str,
        terminals: &[Terminal],
        tabs: Option<&Path>,
        scope: Option<&str>,
    ) -> Layout {
        if let Some((_, Some(spaces))) = &self.doc {
            return spaces.project(boot, terminals, &mut self.names);
        }
        let saved = tabs.and_then(|tabs| self.saved(tabs));
        projection::project(boot, terminals, saved, scope)
    }

    fn saved(&mut self, path: &Path) -> Option<&projection::Saved> {
        let changed = std::fs::metadata(path).and_then(|file| file.modified());
        match (changed, &self.saved) {
            (Ok(at), Some((read, _))) if at == *read => {}
            (Ok(at), _) => self.saved = Some((at, projection::read(path))),
            (Err(_), _) => self.saved = None,
        }
        self.saved.as_ref()?.1.as_ref()
    }
}
