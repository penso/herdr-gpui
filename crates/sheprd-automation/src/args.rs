//! Reading a script's arguments into typed values. Every refusal names the
//! function and field, so `check` can say exactly what to fix.

use crate::{Problem, Refusal, schedule};
use mlua::{Table, Value};
use std::time::Duration;

/// Where an argument came from, for the refusal's message.
#[derive(Clone, Copy)]
pub(crate) struct Site {
    pub(crate) function: &'static str,
}

impl Site {
    pub(crate) fn refuse(self, problem: Problem) -> mlua::Error {
        mlua::Error::external(Refusal::Invalid {
            function: self.function,
            problem,
        })
    }

    /// Refuses any key of `table` that is not in `allowed`.
    pub(crate) fn only(self, table: &Table, allowed: &[&str]) -> mlua::Result<()> {
        for pair in table.pairs::<Value, Value>() {
            let (key, _) = pair?;
            let known = match &key {
                Value::String(name) => allowed
                    .iter()
                    .any(|field| name.as_bytes() == field.as_bytes()),
                _ => false,
            };
            if !known {
                let name = match key {
                    Value::String(name) => name.to_string_lossy(),
                    other => other.type_name().to_owned(),
                };
                return Err(self.refuse(Problem::Unknown(name)));
            }
        }
        Ok(())
    }

    pub(crate) fn text(self, value: Value, rule: Text) -> mlua::Result<String> {
        let Value::String(text) = value else {
            return Err(self.refuse(Problem::Type {
                field: rule.field,
                expected: "a string",
            }));
        };
        let text = text.to_str()?.to_owned();
        rule.check(self, &text)?;
        Ok(text)
    }

    pub(crate) fn required_text(self, table: &Table, rule: Text) -> mlua::Result<String> {
        match table.raw_get::<Value>(rule.field)? {
            Value::Nil => Err(self.refuse(Problem::Missing(rule.field))),
            value => self.text(value, rule),
        }
    }

    pub(crate) fn optional_text(self, table: &Table, rule: Text) -> mlua::Result<Option<String>> {
        match table.raw_get::<Value>(rule.field)? {
            Value::Nil => Ok(None),
            value => self.text(value, rule).map(Some),
        }
    }

    pub(crate) fn optional_bool(
        self,
        table: &Table,
        field: &'static str,
    ) -> mlua::Result<Option<bool>> {
        match table.raw_get::<Value>(field)? {
            Value::Nil => Ok(None),
            Value::Boolean(value) => Ok(Some(value)),
            _ => Err(self.refuse(Problem::Type {
                field,
                expected: "true or false",
            })),
        }
    }

    /// A whole number in `min..=max`, from a Luau number.
    pub(crate) fn whole(
        self,
        table: &Table,
        field: &'static str,
        min: u64,
        max: u64,
        range: &'static str,
    ) -> mlua::Result<u64> {
        let number = match table.raw_get::<Value>(field)? {
            Value::Nil => return Err(self.refuse(Problem::Missing(field))),
            Value::Integer(value) => value as f64,
            Value::Number(value) => value,
            _ => {
                return Err(self.refuse(Problem::Type {
                    field,
                    expected: "a number",
                }));
            }
        };
        if number.fract() != 0. || number < min as f64 || number > max as f64 {
            return Err(self.refuse(Problem::Range { field, range }));
        }
        Ok(number as u64)
    }

    /// A duration such as `30m`, between `min` and `max`.
    pub(crate) fn duration(
        self,
        value: Value,
        field: &'static str,
        min: Duration,
        max: Duration,
        range: &'static str,
    ) -> mlua::Result<Duration> {
        let text = self.text(value, Text::name(field))?;
        let duration =
            schedule::parse_duration(&text).ok_or_else(|| self.refuse(Problem::Duration(text)))?;
        if duration < min || duration > max {
            return Err(self.refuse(Problem::Range { field, range }));
        }
        Ok(duration)
    }

    pub(crate) fn optional_duration(
        self,
        table: &Table,
        field: &'static str,
        min: Duration,
        max: Duration,
        range: &'static str,
    ) -> mlua::Result<Option<Duration>> {
        match table.raw_get::<Value>(field)? {
            Value::Nil => Ok(None),
            value => self.duration(value, field, min, max, range).map(Some),
        }
    }
}

/// How a text argument is bounded.
#[derive(Clone, Copy)]
pub(crate) struct Text {
    pub(crate) field: &'static str,
    limit: usize,
    form: Form,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Form {
    /// One line of any visible text: a trigger name, a time zone.
    Line,
    /// Free text that may span lines and carry tokens: a prompt, a command.
    Body,
    /// A git branch or worktree name: `A-Z a-z 0-9 . _ / -` and `{date}`.
    Reference,
    /// A lowercase name such as an agent harness: `a-z 0-9 -`.
    Identifier,
}

impl Text {
    pub(crate) const fn name(field: &'static str) -> Self {
        Self {
            field,
            limit: 200,
            form: Form::Line,
        }
    }

    pub(crate) const fn body(field: &'static str, limit: usize) -> Self {
        Self {
            field,
            limit,
            form: Form::Body,
        }
    }

    /// A branch or worktree name. Step output tokens cannot appear here;
    /// only `{date}`, which fills in as digits and dashes.
    pub(crate) const fn reference(field: &'static str) -> Self {
        Self {
            field,
            limit: 200,
            form: Form::Reference,
        }
    }

    pub(crate) const fn identifier(field: &'static str) -> Self {
        Self {
            field,
            limit: 64,
            form: Form::Identifier,
        }
    }

    fn check(self, site: Site, text: &str) -> mlua::Result<()> {
        if text.trim().is_empty() {
            return Err(site.refuse(Problem::Empty(self.field)));
        }
        if text.len() > self.limit {
            return Err(site.refuse(Problem::TooLong {
                field: self.field,
                limit: self.limit,
            }));
        }
        let allowed = |c: char| self.form == Form::Body && matches!(c, '\n' | '\t');
        if text.chars().any(|c| c.is_control() && !allowed(c)) {
            return Err(site.refuse(Problem::Control(self.field)));
        }
        if text.chars().any(invisible) {
            return Err(site.refuse(Problem::Format(self.field)));
        }
        let fits = match self.form {
            Form::Line | Form::Body => true,
            Form::Reference => is_reference(text),
            Form::Identifier => text
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
        };
        if !fits {
            return Err(site.refuse(Problem::Shape {
                field: self.field,
                rule: match self.form {
                    Form::Identifier => "lowercase letters, digits and -",
                    _ => "a branch name: letters, digits, . _ / -, and {date}; no leading -, no ..",
                },
            }));
        }
        Ok(())
    }
}

/// A name git accepts as a branch, also once `{date}` is filled in.
fn is_reference(text: &str) -> bool {
    let filled = text.replace(crate::tokens::DATE, "2026-01-01");
    let plain = filled
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '/' | '-'));
    plain
        && !filled.starts_with(['-', '/', '.'])
        && !filled.ends_with(['/', '.'])
        && !filled.ends_with(".lock")
        && !filled.contains("..")
        && !filled.contains("//")
        && !filled.contains("/.")
}

/// A line as it must be reviewed: in full, with control and invisible
/// characters written as `\u{…}` so a terminal or label cannot hide or
/// restyle them.
pub fn visible(line: &str) -> String {
    line.chars()
        .map(|c| {
            if (c.is_control() && c != '\t') || invisible(c) {
                format!("\\u{{{:04x}}}", u32::from(c))
            } else {
                c.to_string()
            }
        })
        .collect()
}

/// Characters that change how text reads without showing, or show as
/// anything: format controls such as bidi overrides and zero-width joiners,
/// line and paragraph separators, private use, unassigned code points, and
/// the default-ignorable marks outside those categories. A plan must read
/// as what runs.
pub fn invisible(c: char) -> bool {
    use unicode_properties::{GeneralCategory, UnicodeGeneralCategory};
    matches!(
        c.general_category(),
        GeneralCategory::Format
            | GeneralCategory::LineSeparator
            | GeneralCategory::ParagraphSeparator
            | GeneralCategory::PrivateUse
            | GeneralCategory::Surrogate
            | GeneralCategory::Unassigned
    ) || matches!(
        c,
        '\u{034f}'
            | '\u{115f}'
            | '\u{1160}'
            | '\u{17b4}'
            | '\u{17b5}'
            | '\u{180b}'..='\u{180f}'
            | '\u{3164}'
            | '\u{fe00}'..='\u{fe0f}'
            | '\u{ffa0}'
            | '\u{e0000}'..='\u{e0fff}'
    )
}
