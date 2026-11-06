//! The session's modes and settings: Codex's approval presets, and the
//! models and reasoning efforts `model/list` offers. Codex takes them with
//! each turn, so a change waits for no answer.

use acp::v1 as wire;
use serde_json::{Value, json};

use crate::harness::out;

pub(super) const MODEL: &str = "model";
pub(super) const EFFORT: &str = "effort";
/// Models kept from `model/list`, so a long list cannot grow the session.
const MAX_MODELS: usize = 64;

/// Codex's approval presets, as its own terminal view offers them.
const MODES: [(&str, &str, &str); 3] = [
    (
        "read-only",
        "Read Only",
        "Read files; ask before any change or command",
    ),
    (
        "auto",
        "Auto",
        "Edit and run in the workspace; ask before going further",
    ),
    (
        "full-access",
        "Full Access",
        "Edit and run anywhere, network included, without asking",
    ),
];
const DEFAULT_MODE: &str = "auto";

#[derive(Clone, Debug)]
struct Model {
    /// The model `model/list` marks as the default.
    default: bool,
    default_effort: Option<String>,
    description: Option<String>,
    efforts: Vec<String>,
    id: String,
    name: String,
}

#[derive(Clone, Debug, Default)]
pub(super) struct Settings {
    effort: Option<String>,
    mode: String,
    model: Option<String>,
    models: Vec<Model>,
}

impl Settings {
    /// From `model/list`, with the model and effort the thread started with.
    pub(super) fn new(models: &Value, thread: &Value) -> Self {
        let models: Vec<Model> = models
            .get("data")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|model| model.get("hidden") != Some(&Value::Bool(true)))
            .filter_map(|model| {
                let text = |key: &str| model.get(key).and_then(Value::as_str).map(str::to_owned);
                Some(Model {
                    default: model.get("isDefault") == Some(&Value::Bool(true)),
                    default_effort: text("defaultReasoningEffort"),
                    description: text("description"),
                    efforts: model
                        .get("supportedReasoningEfforts")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(|effort| effort.get("reasoningEffort").and_then(Value::as_str))
                        .map(str::to_owned)
                        .collect(),
                    name: text("displayName").or_else(|| text("id"))?,
                    id: text("id")?,
                })
            })
            .take(MAX_MODELS)
            .collect();
        let started = thread.get("model").and_then(Value::as_str);
        // The thread's model when Codex offers it; else the one it marks the
        // default, since a configured model may not be one this account has.
        let model = started
            .filter(|started| models.iter().any(|model| model.id == *started))
            .map(str::to_owned)
            .or_else(|| {
                models
                    .iter()
                    .find(|model| model.default)
                    .or_else(|| models.first())
                    .map(|model| model.id.clone())
            });
        let mut settings = Self {
            effort: thread
                .get("reasoningEffort")
                .and_then(Value::as_str)
                .map(str::to_owned),
            mode: DEFAULT_MODE.to_owned(),
            model,
            models,
        };
        settings.keep_effort();
        settings
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
        if let Some(model) = &self.model {
            options.push(out::select(
                MODEL,
                "Model",
                wire::SessionConfigOptionCategory::Model,
                model,
                self.models.iter().map(|model| {
                    (
                        model.id.clone(),
                        model.name.clone(),
                        model.description.clone(),
                    )
                }),
            ));
        }
        if let (Some(effort), Some(model)) = (&self.effort, self.current()) {
            options.push(out::select(
                EFFORT,
                "Reasoning",
                wire::SessionConfigOptionCategory::ThoughtLevel,
                effort,
                model
                    .efforts
                    .iter()
                    .map(|level| (level.clone(), capitalized(level), None)),
            ));
        }
        options
    }

    fn current(&self) -> Option<&Model> {
        self.models
            .iter()
            .find(|model| Some(&model.id) == self.model.as_ref())
    }

    /// Picks `value` for `setting`; `false` when it is not one offered.
    pub(super) fn set(&mut self, setting: &str, value: &str) -> bool {
        match setting {
            MODEL if self.models.iter().any(|model| model.id == value) => {
                self.model = Some(value.to_owned());
                self.keep_effort();
                true
            }
            EFFORT
                if self
                    .current()
                    .is_some_and(|model| model.efforts.iter().any(|known| known == value)) =>
            {
                self.effort = Some(value.to_owned());
                true
            }
            _ => false,
        }
    }

    pub(super) fn set_mode(&mut self, mode: &str) -> bool {
        if !MODES.iter().any(|(id, ..)| *id == mode) {
            return false;
        }
        mode.clone_into(&mut self.mode);
        true
    }

    /// Keeps an effort the model takes, else its default.
    fn keep_effort(&mut self) {
        let Some(model) = self.current() else {
            self.effort = None;
            return;
        };
        if self
            .effort
            .as_ref()
            .is_some_and(|effort| model.efforts.contains(effort))
        {
            return;
        }
        self.effort = model
            .default_effort
            .clone()
            .filter(|effort| model.efforts.contains(effort))
            .or_else(|| model.efforts.first().cloned());
    }

    /// What `turn/start` carries for the chosen settings.
    pub(super) fn turn(&self) -> serde_json::Map<String, Value> {
        let (approval, sandbox) = match self.mode.as_str() {
            "read-only" => (
                "on-request",
                json!({"type": "readOnly", "networkAccess": false}),
            ),
            "full-access" => ("never", json!({"type": "dangerFullAccess"})),
            _ => (
                "on-request",
                json!({
                    "type": "workspaceWrite",
                    "writableRoots": [],
                    "networkAccess": false,
                    "excludeTmpdirEnvVar": false,
                    "excludeSlashTmp": false,
                }),
            ),
        };
        let mut fields = serde_json::Map::new();
        fields.insert("approvalPolicy".to_owned(), json!(approval));
        fields.insert("sandboxPolicy".to_owned(), sandbox);
        if let Some(model) = &self.model {
            fields.insert("model".to_owned(), json!(model));
        }
        if let Some(effort) = &self.effort {
            fields.insert("effort".to_owned(), json!(effort));
        }
        fields
    }
}

fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}
