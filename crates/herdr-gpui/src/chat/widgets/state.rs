//! Per-pane form values and native input entities. Merely displaying a card
//! allocates no retained state; the first user edit creates it.

use super::{Document, Values};
use crate::{chat::prepared::Prepared, search_input::SearchInput};
use gpui::{App, Entity, Subscription, Window};
use herdr_chat_protocol::EntryId;
use std::{collections::BTreeMap, rc::Rc};

pub(crate) type Key = (EntryId, usize);
const MAX_FORMS: usize = 64;

pub(crate) struct Input {
    pub(crate) entity: Entity<SearchInput>,
    pub(crate) _changed: Subscription,
}

pub(crate) struct Form {
    pub(crate) document: Rc<Document>,
    pub(crate) values: Values,
    pub(crate) inputs: BTreeMap<String, Input>,
}

#[derive(Default)]
pub(crate) struct State {
    forms: BTreeMap<Key, Form>,
}

impl State {
    pub(crate) fn clear(&mut self) {
        self.forms.clear();
    }

    pub(crate) fn retain(&mut self, prepared: &Prepared) {
        // Invalidate on the update that replaces or removes a card, even if
        // nobody edits its replacement before the old source appears again.
        self.forms
            .retain(|key, form| prepared.shows_widget(*key, &form.document));
    }

    pub(crate) fn form(&self, key: Key, document: &Document) -> Option<&Form> {
        self.forms
            .get(&key)
            .filter(|form| form.document.source == document.source)
    }

    pub(crate) fn edit(&mut self, key: Key, document: &Rc<Document>) -> &mut Form {
        if self
            .forms
            .get(&key)
            .is_some_and(|form| form.document.source != document.source)
        {
            self.forms.remove(&key);
        }
        if !self.forms.contains_key(&key) && self.forms.len() >= MAX_FORMS {
            self.forms.pop_first();
        }
        self.forms.entry(key).or_insert_with(|| Form {
            document: document.clone(),
            values: document.defaults(),
            inputs: BTreeMap::new(),
        })
    }

    pub(crate) fn focused(&self, window: &Window, cx: &App) -> bool {
        self.forms
            .values()
            .flat_map(|form| form.inputs.values())
            .any(|input| input.entity.read(cx).focus.is_focused(window))
    }
}

pub(crate) fn bounded_input(value: &str) -> String {
    let mut result = String::new();
    for ch in value
        .chars()
        .filter(|ch| !crate::notifications::unsafe_char(*ch))
    {
        if result.len() + ch.len_utf8() > super::MAX_INPUT {
            break;
        }
        result.push(ch);
    }
    result
}
