//! The session's modes and settings as ACP shows them: Claude Code's
//! permission modes, and the models and effort levels its handshake lists.

use acp::v1 as wire;
use serde_json::Value;

pub(super) const MODEL: &str = "model";
pub(super) const EFFORT: &str = "effort";
/// The effort choice that leaves the model's own default.
const DEFAULT_EFFORT: &str = "default";
/// Models kept from the handshake, so a long list cannot grow the session.
const MAX_MODELS: usize = 64;

/// Claude Code's permission modes, in the order they cycle.
const MODES: [(&str, &str, &str); 4] = [
    ("default", "Default", "Ask before edits and commands"),
    ("acceptEdits", "Accept Edits", "Edit files without asking"),
    ("plan", "Plan", "Explore and plan without changing anything"),
    (
        "bypassPermissions",
        "Bypass Permissions",
        "Run everything without asking",
    ),
];

#[derive(Clone, Debug, Default)]
struct Model {
    description: Option<String>,
    efforts: Vec<String>,
    name: String,
    /// The model id this choice runs now, as `system/init` names it.
    resolved: Option<String>,
    value: String,
}

#[derive(Clone, Debug)]
pub(super) struct Settings {
    effort: String,
    mode: String,
    model: String,
    models: Vec<Model>,
}

impl Settings {
    /// From the `initialize` control response.
    pub(super) fn from_handshake(response: &Value) -> Self {
        let models = response
            .get("models")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|model| {
                let text = |key: &str| model.get(key).and_then(Value::as_str).map(str::to_owned);
                Some(Model {
                    description: text("description"),
                    efforts: model
                        .get("supportedEffortLevels")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect(),
                    name: text("displayName").or_else(|| text("value"))?,
                    resolved: text("resolvedModel"),
                    value: text("value")?,
                })
            })
            .take(MAX_MODELS)
            .collect::<Vec<_>>();
        let model = models
            .first()
            .map_or_else(|| "default".to_owned(), |model| model.value.clone());
        let mode = response
            .get("current_permission_mode")
            .and_then(Value::as_str)
            .filter(|mode| known_mode(mode))
            .unwrap_or("default")
            .to_owned();
        Self {
            effort: DEFAULT_EFFORT.to_owned(),
            mode,
            model,
            models,
        }
    }

    pub(super) fn modes(&self) -> wire::SessionModeState {
        wire::SessionModeState::new(
            self.mode.clone(),
            MODES
                .iter()
                .map(|(id, name, description)| {
                    wire::SessionMode::new(*id, *name).description(Some((*description).to_owned()))
                })
                .collect(),
        )
    }

    pub(super) fn options(&self) -> Vec<wire::SessionConfigOption> {
        let mut options = Vec::with_capacity(2);
        if !self.models.is_empty() {
            let choices: Vec<_> = self
                .models
                .iter()
                .map(|model| {
                    wire::SessionConfigSelectOption::new(model.value.clone(), model.name.clone())
                        .description(model.description.clone())
                })
                .collect();
            options.push(
                wire::SessionConfigOption::select(MODEL, "Model", self.model.clone(), choices)
                    .category(Some(wire::SessionConfigOptionCategory::Model)),
            );
        }
        let efforts = self.efforts();
        if !efforts.is_empty() {
            let mut choices = vec![wire::SessionConfigSelectOption::new(
                DEFAULT_EFFORT,
                "Default",
            )];
            choices.extend(efforts.iter().map(|level| {
                wire::SessionConfigSelectOption::new(level.clone(), capitalized(level))
            }));
            options.push(
                wire::SessionConfigOption::select(EFFORT, "Effort", self.effort.clone(), choices)
                    .category(Some(wire::SessionConfigOptionCategory::ThoughtLevel)),
            );
        }
        options
    }

    /// The effort levels the chosen model takes.
    fn efforts(&self) -> &[String] {
        self.models
            .iter()
            .find(|model| model.value == self.model)
            .map_or(&[], |model| &model.efforts)
    }

    pub(super) fn mode(&self) -> &str {
        &self.mode
    }

    /// Records the mode Claude now runs in; `false` when nothing changed.
    pub(super) fn set_mode(&mut self, mode: &str) -> bool {
        if self.mode == mode || !known_mode(mode) {
            return false;
        }
        mode.clone_into(&mut self.mode);
        true
    }

    pub(super) fn has_model(&self, value: &str) -> bool {
        self.models.iter().any(|model| model.value == value)
    }

    pub(super) fn takes_effort(&self, level: &str) -> bool {
        level == DEFAULT_EFFORT || self.efforts().iter().any(|known| known == level)
    }

    pub(super) fn set_model(&mut self, value: &str) {
        value.clone_into(&mut self.model);
        if !self.takes_effort(&self.effort.clone()) {
            DEFAULT_EFFORT.clone_into(&mut self.effort);
        }
    }

    pub(super) fn set_effort(&mut self, level: &str) {
        level.clone_into(&mut self.effort);
    }

    /// Follows a model Claude switched to by itself, such as through
    /// `/model`, when the chosen one no longer runs it; `false` when the
    /// choice already does.
    pub(super) fn observe_model(&mut self, resolved: &str) -> bool {
        let runs = |model: &Model| model.resolved.as_deref() == Some(resolved);
        if self
            .models
            .iter()
            .any(|model| model.value == self.model && runs(model))
        {
            return false;
        }
        let Some(value) = self
            .models
            .iter()
            .find(|model| runs(model))
            .map(|model| model.value.clone())
        else {
            return false;
        };
        self.set_model(&value);
        true
    }
}

/// The `apply_flag_settings` value for an effort choice.
pub(super) fn effort_setting(level: &str) -> Value {
    if level == DEFAULT_EFFORT {
        Value::Null
    } else {
        Value::String(level.to_owned())
    }
}

fn known_mode(mode: &str) -> bool {
    MODES.iter().any(|(id, ..)| *id == mode)
}

fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}
