//! The dispatch dialog's Arguments field: what the picked agent starts
//! with after its executable. It opens on the arguments this view last
//! dispatched that agent with, else the config's (`[orchestrator.agents]`),
//! keeps what was typed for each agent while the dialog is open, and is
//! split as a POSIX shell splits words, without running one, so the agent
//! gets each argument as written.

use super::OrchestratorView;
use crate::orchestrator::actions::{MAX_ARG_BYTES, MAX_ARGS, valid_args};
use crate::{config::OrchestratorConfig, teleport::AgentKind};
use gpui::Context;

// The problem text below names these limits.
const _: () = assert!(MAX_ARGS == 32 && MAX_ARG_BYTES == 1024);

/// The arguments last dispatched with, by agent and by whether it was a
/// review, for the next dialog to open on.
#[derive(Default)]
pub(crate) struct Used(Vec<(AgentKind, bool, String)>);

impl OrchestratorView {
    /// Takes the config's agent arguments, which new dialogs open on.
    pub(crate) fn set_agent_config(&mut self, config: &OrchestratorConfig) {
        if self.agent_config != *config {
            self.agent_config = config.clone();
        }
    }

    /// The field's text for `kind` in a new dialog.
    fn default_args(&self, kind: AgentKind, review: bool) -> String {
        if let Some((.., text)) = self
            .used_args
            .0
            .iter()
            .find(|(used, was_review, _)| *used == kind && *was_review == review)
        {
            return text.clone();
        }
        let args = self.agent_config.args(kind, review);
        // Config arguments are free of NUL, the one thing that cannot be quoted.
        shlex::try_join(args.iter().map(String::as_str)).unwrap_or_default()
    }

    /// Makes `kind` the dialog's agent. The field then shows that agent's
    /// arguments, and keeps what it held for the agent it showed before
    /// should that one be picked again. No agent, as while a host's agents
    /// load, leaves the field as it is.
    pub(in crate::orchestrator::view) fn pick_agent(
        &mut self,
        kind: Option<AgentKind>,
        cx: &mut Context<Self>,
    ) {
        let Some(review) = self.dialog.as_ref().map(|dialog| dialog.review) else {
            return;
        };
        let fallback = kind.map(|kind| self.default_args(kind, review));
        let Some(dialog) = &mut self.dialog else {
            return;
        };
        dialog.kind = kind;
        let (Some(kind), Some(fallback)) = (kind, fallback) else {
            return;
        };
        if dialog.args_kind == Some(kind) {
            return;
        }
        let typed = dialog.args.read(cx).text().to_owned();
        if let Some(shown) = dialog.args_kind.replace(kind) {
            dialog.drafts.retain(|(agent, _)| *agent != shown);
            dialog.drafts.push((shown, typed));
        }
        let text = dialog
            .drafts
            .iter()
            .find(|(agent, _)| *agent == kind)
            .map_or(fallback, |(_, text)| text.clone());
        dialog
            .args
            .update(cx, |input, cx| input.set_text_at(&text, text.len(), cx));
    }

    /// Remembers what `kind` was dispatched with, for the next dialog.
    pub(super) fn remember_args(&mut self, kind: AgentKind, review: bool, text: String) {
        let used = &mut self.used_args.0;
        used.retain(|(agent, was_review, _)| (*agent, *was_review) != (kind, review));
        used.push((kind, review, text));
    }
}

/// The field's `text` as arguments, or what to tell the user.
pub(super) fn parse_args(text: &str) -> Result<Vec<String>, &'static str> {
    let args = shlex::split(text).ok_or("An argument has a quote that is never closed.")?;
    if !valid_args(&args) {
        return Err("Arguments are limited to 32, of at most 1 KiB each.");
    }
    Ok(args)
}
