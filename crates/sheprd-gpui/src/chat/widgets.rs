//! A small, declarative native UI catalog. Agent JSON is data: it cannot name
//! code, URLs, tools, or commands. Only an explicit action click prepares a
//! normal chat prompt containing the user's current form values.

mod error;
mod state;
pub(crate) use error::Error;
pub(crate) use state::{Form, Input, Key, State, bounded_input};

use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, rc::Rc};

pub(crate) const MAX_SOURCE: usize = 32 * 1024;
pub(crate) const MAX_INPUT: usize = 2048;
const MAX_COMPONENTS: usize = 32;
const MAX_FIELDS: usize = 16;
const MAX_PROMPT: usize = 16 * 1024;
/// Bytes of a title, label, option, column, or chart point name.
const MAX_LABEL: usize = 160;
/// Bytes of a card's or a text component's body.
const MAX_BODY: usize = 8192;
/// Bytes of the prompt an action starts.
const MAX_ACTION_PROMPT: usize = 4096;
/// Bytes of a metric's value.
const MAX_METRIC: usize = 128;
/// Bytes of a metric's detail or a table cell.
const MAX_DETAIL: usize = 512;
/// Bytes of a field id.
const MAX_FIELD_ID: usize = 64;
const MAX_CHOICES: usize = 12;
const MAX_COLUMNS: usize = 8;
const MAX_ROWS: usize = 32;
const MAX_POINTS: usize = 32;
/// Largest magnitude of a slider bound, step, value, or chart point.
const MAX_NUMBER: f64 = 1e9;
/// Most steps a slider may have between its bounds.
const MAX_STEPS: f64 = 10_000.;

#[derive(Debug)]
pub(crate) struct Document {
    pub(crate) title: String,
    pub(crate) components: Vec<Component>,
    pub(crate) source: Rc<str>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Spec {
    #[serde(default)]
    title: String,
    components: Vec<Component>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Component {
    Action {
        label: String,
        prompt: String,
        #[serde(default)]
        mode: ActionMode,
    },
    Card {
        title: String,
        text: String,
    },
    Chart {
        label: String,
        points: Vec<Point>,
    },
    Choice {
        id: String,
        label: String,
        options: Vec<String>,
        #[serde(default)]
        value: String,
    },
    Input {
        id: String,
        label: String,
        #[serde(default)]
        placeholder: String,
        #[serde(default)]
        value: String,
    },
    Metric {
        label: String,
        value: String,
        #[serde(default)]
        detail: String,
    },
    Slider {
        id: String,
        label: String,
        min: f64,
        max: f64,
        #[serde(default = "one")]
        step: f64,
        value: f64,
    },
    Table {
        columns: Vec<String>,
        rows: Vec<Vec<String>>,
    },
    Text {
        text: String,
    },
    Toggle {
        id: String,
        label: String,
        #[serde(default)]
        value: bool,
    },
}

fn one() -> f64 {
    1.
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ActionMode {
    #[default]
    Draft,
    Send,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Point {
    pub(crate) label: String,
    pub(crate) value: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub(crate) enum Value {
    Text(String),
    Number(f64),
    Toggle(bool),
}

pub(crate) type Values = BTreeMap<String, Value>;

fn text(value: &str, max: usize) -> bool {
    value.len() <= max
        && !value
            .chars()
            .any(|ch| crate::notifications::unsafe_char(ch) && !matches!(ch, '\n' | '\t'))
}

impl Component {
    pub(crate) fn field(&self) -> Option<(&str, Value)> {
        match self {
            Self::Choice { id, value, .. } | Self::Input { id, value, .. } => {
                Some((id, Value::Text(value.clone())))
            }
            Self::Slider { id, value, .. } => Some((id, Value::Number(*value))),
            Self::Toggle { id, value, .. } => Some((id, Value::Toggle(*value))),
            _ => None,
        }
    }

    /// Whether the component is drawable, with a default value its own
    /// control would accept as an edit.
    fn valid(&self) -> bool {
        self.well_formed() && self.field().is_none_or(|(_, value)| self.accepts(&value))
    }

    fn well_formed(&self) -> bool {
        match self {
            Self::Action { label, prompt, .. } => {
                text(label, MAX_LABEL)
                    && !label.is_empty()
                    && text(prompt, MAX_ACTION_PROMPT)
                    && !prompt.trim().is_empty()
            }
            Self::Card { title, text: body } => text(title, MAX_LABEL) && text(body, MAX_BODY),
            Self::Text { text: body } => text(body, MAX_BODY),
            Self::Metric {
                label,
                value,
                detail,
            } => text(label, MAX_LABEL) && text(value, MAX_METRIC) && text(detail, MAX_DETAIL),
            Self::Choice { label, options, .. } => {
                text(label, MAX_LABEL)
                    && (1..=MAX_CHOICES).contains(&options.len())
                    && options
                        .iter()
                        .all(|option| !option.is_empty() && text(option, MAX_LABEL))
                    && options
                        .iter()
                        .collect::<std::collections::HashSet<_>>()
                        .len()
                        == options.len()
            }
            Self::Toggle { label, .. } => text(label, MAX_LABEL),
            Self::Input {
                label, placeholder, ..
            } => text(label, MAX_LABEL) && text(placeholder, MAX_LABEL),
            Self::Slider {
                label,
                min,
                max,
                step,
                ..
            } => {
                text(label, MAX_LABEL)
                    && [min, max, step]
                        .iter()
                        .all(|n| n.is_finite() && n.abs() <= MAX_NUMBER)
                    && max > min
                    && *step > 0.
                    && *step <= max - min
                    && (max - min) / step <= MAX_STEPS
            }
            Self::Table { columns, rows } => {
                (1..=MAX_COLUMNS).contains(&columns.len())
                    && rows.len() <= MAX_ROWS
                    && columns.iter().all(|cell| text(cell, MAX_LABEL))
                    && rows.iter().all(|row| {
                        row.len() == columns.len() && row.iter().all(|cell| text(cell, MAX_DETAIL))
                    })
            }
            Self::Chart { label, points } => {
                text(label, MAX_LABEL)
                    && (1..=MAX_POINTS).contains(&points.len())
                    && points.iter().all(|point| {
                        text(&point.label, MAX_LABEL)
                            && point.value.is_finite()
                            && (0. ..=MAX_NUMBER).contains(&point.value)
                    })
            }
        }
    }

    pub(crate) fn accepts(&self, value: &Value) -> bool {
        match (self, value) {
            (Self::Choice { options, .. }, Value::Text(value)) => options.contains(value),
            (Self::Input { .. }, Value::Text(value)) => {
                text(value, MAX_INPUT) && !value.contains('\n')
            }
            (Self::Toggle { .. }, Value::Toggle(_)) => true,
            (Self::Slider { min, max, .. }, Value::Number(value)) => {
                value.is_finite() && (min..=max).contains(&value)
            }
            _ => false,
        }
    }
}

impl Document {
    pub(crate) fn parse(source: &str) -> Result<Self, Error> {
        if source.len() > MAX_SOURCE {
            return Err(Error::TooLarge);
        }
        let mut spec: Spec = serde_json::from_str(source)?;
        if !text(&spec.title, MAX_LABEL) {
            return Err(Error::Title);
        }
        if !(1..=MAX_COMPONENTS).contains(&spec.components.len()) {
            return Err(Error::ComponentCount);
        }
        let mut fields = std::collections::HashSet::new();
        for component in &mut spec.components {
            if let Component::Choice { options, value, .. } = component
                && value.is_empty()
                && let Some(first) = options.first()
            {
                *value = first.clone();
            }
            if !component.valid() {
                return Err(Error::Component);
            }
            if let Some((id, _)) = component.field()
                && (id.is_empty()
                    || id.len() > MAX_FIELD_ID
                    || !id
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-'))
                    || !fields.insert(id.to_owned()))
            {
                return Err(Error::FieldId);
            }
        }
        if fields.len() > MAX_FIELDS {
            return Err(Error::TooManyFields);
        }
        Ok(Self {
            title: spec.title,
            components: spec.components,
            source: source.into(),
        })
    }

    pub(crate) fn defaults(&self) -> Values {
        self.components
            .iter()
            .filter_map(Component::field)
            .map(|(id, value)| (id.to_owned(), value))
            .collect()
    }

    pub(crate) fn set(&self, values: &mut Values, id: &str, value: Value) -> bool {
        let valid = self.components.iter().any(|component| {
            component.field().is_some_and(|(field, _)| field == id) && component.accepts(&value)
        });
        if valid {
            values.insert(id.to_owned(), value);
        }
        valid
    }

    /// Actions include exactly this catalog's fields, never arbitrary keys a
    /// stale event or caller supplied. JSON preserves literal user text.
    pub(crate) fn prompt(&self, prefix: &str, values: &Values) -> Result<String, Error> {
        let mut checked = self.defaults();
        for (id, value) in values {
            if !self.set(&mut checked, id, value.clone()) {
                return Err(Error::FormValue);
            }
        }
        if !self.components.iter().any(
            |component| matches!(component, Component::Action { prompt, .. } if prompt == prefix),
        ) {
            return Err(Error::ActionGone);
        }
        let prompt = if checked.is_empty() {
            prefix.to_owned()
        } else {
            format!(
                "{prefix}\n\nSelected values:\n{}",
                serde_json::to_string_pretty(&checked)?
            )
        };
        if prompt.len() > MAX_PROMPT {
            return Err(Error::TooLarge);
        }
        Ok(prompt)
    }
}

/// Slider gestures are local state only; the value leaves the view solely
/// through an action click. Clamping also handles the last fractional step.
pub(crate) fn slider_value(fraction: f64, min: f64, max: f64, step: f64) -> f64 {
    if fraction <= 0. {
        return min;
    }
    if fraction >= 1. {
        return max;
    }
    snap(min + fraction * (max - min), min, max, step)
}

/// Rounds a slider value to the step grid from `min`, then to the decimals
/// `min` and `step` are written with, so `0.05 * 7` reads `0.35` in the label
/// and the prompt JSON rather than carrying binary float noise.
pub(crate) fn snap(value: f64, min: f64, max: f64, step: f64) -> f64 {
    let decimals = |number: f64| {
        let text = number.abs().to_string();
        text.split_once('.')
            .map_or(0, |(_, fraction)| fraction.len())
    };
    let scale = 10f64.powi(decimals(min).max(decimals(step)).min(12) as i32);
    let stepped = min + ((value - min) / step).round() * step;
    let rounded = (stepped * scale).round() / scale;
    if rounded.is_finite() {
        rounded
    } else {
        stepped
    }
    .clamp(min, max)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
