//! A cursor over a control command's arguments. Values stay OS strings until
//! a command asks for UTF-8, so a path argument never has to be one.
use super::CliError;
use std::{collections::VecDeque, ffi::OsString};

pub(crate) struct Args {
    rest: VecDeque<OsString>,
    /// Everything after a `--`, taken as it is: no flag there is this
    /// program's, as in `cloud exec ID -- git log --json`.
    trailing: Option<Vec<OsString>>,
    /// The command being parsed, named in errors.
    command: &'static str,
}

fn is_flag(value: &OsString) -> bool {
    value.as_encoded_bytes().starts_with(b"-") && value.len() > 1
}

impl Args {
    pub(crate) fn new(args: impl IntoIterator<Item = OsString>, command: &'static str) -> Self {
        let mut rest: VecDeque<OsString> = args.into_iter().collect();
        let trailing = rest
            .iter()
            .position(|arg| arg == "--")
            .map(|at| rest.split_off(at).into_iter().skip(1).collect());
        Self {
            rest,
            trailing,
            command,
        }
    }

    /// What followed `--`, once; `None` when there was no `--`.
    pub(crate) fn trailing(&mut self) -> Option<Vec<OsString>> {
        self.trailing.take()
    }

    pub(crate) fn name(&self) -> &'static str {
        self.command
    }

    /// Everything left, in order, for a parser of its own.
    pub(crate) fn take_all(&mut self) -> impl Iterator<Item = OsString> + use<> {
        std::mem::take(&mut self.rest).into_iter()
    }

    /// The next word, which names a subcommand.
    pub(crate) fn verb(&mut self) -> Option<OsString> {
        self.rest.pop_front()
    }

    /// Whether `--help` or `-h` appears anywhere, which wins over every
    /// other argument as it does for the browser commands.
    pub(crate) fn wants_help(&self) -> bool {
        self.rest.iter().any(|arg| arg == "--help" || arg == "-h")
    }

    /// Removes `--flag` wherever it is; whether it was there.
    pub(crate) fn flag(&mut self, flag: &str) -> bool {
        let before = self.rest.len();
        self.rest.retain(|arg| arg != flag);
        self.rest.len() != before
    }

    /// Removes `--flag VALUE` wherever it is. A value may not look like a flag.
    pub(crate) fn option(&mut self, flag: &'static str) -> Result<Option<OsString>, CliError> {
        let Some(at) = self.rest.iter().position(|arg| arg == flag) else {
            return Ok(None);
        };
        let _ = self.rest.remove(at);
        let value = self
            .rest
            .remove(at)
            .filter(|value| !value.is_empty() && !is_flag(value))
            .ok_or(CliError::MissingValue(flag))?;
        if self.rest.iter().any(|arg| arg == flag) {
            return Err(CliError::DuplicateOption(flag));
        }
        Ok(Some(value))
    }

    /// Every `--flag VALUE`, for a flag that may repeat.
    pub(crate) fn options(&mut self, flag: &'static str) -> Result<Vec<OsString>, CliError> {
        let mut values = Vec::new();
        while let Some(at) = self.rest.iter().position(|arg| arg == flag) {
            let _ = self.rest.remove(at);
            values.push(
                self.rest
                    .remove(at)
                    .filter(|value| !value.is_empty() && !is_flag(value))
                    .ok_or(CliError::MissingValue(flag))?,
            );
        }
        Ok(values)
    }

    pub(crate) fn text_option(&mut self, flag: &'static str) -> Result<Option<String>, CliError> {
        self.option(flag)?.map(utf8).transpose()
    }

    /// `--flag N`, a whole number within `range`.
    pub(crate) fn number<T>(
        &mut self,
        flag: &'static str,
        range: std::ops::RangeInclusive<T>,
    ) -> Result<Option<T>, CliError>
    where
        T: std::str::FromStr + PartialOrd,
    {
        let Some(value) = self.option(flag)? else {
            return Ok(None);
        };
        value
            .to_str()
            .and_then(|text| text.parse::<T>().ok())
            .filter(|number| range.contains(number))
            .map(Some)
            .ok_or(CliError::InvalidValue(flag))
    }

    /// The next positional argument, which may not look like a flag.
    pub(crate) fn positional(&mut self) -> Option<OsString> {
        let at = self.rest.iter().position(|arg| !is_flag(arg))?;
        self.rest.remove(at)
    }

    pub(crate) fn required(&mut self, what: &'static str) -> Result<OsString, CliError> {
        self.positional().ok_or(CliError::MissingArgument {
            command: self.command,
            what,
        })
    }

    pub(crate) fn required_text(&mut self, what: &'static str) -> Result<String, CliError> {
        utf8(self.required(what)?)
    }

    pub(crate) fn optional_text(&mut self) -> Result<Option<String>, CliError> {
        self.positional().map(utf8).transpose()
    }

    /// Every remaining positional argument joined by spaces, as a prompt or
    /// a note is typed unquoted.
    pub(crate) fn words(&mut self) -> Result<Option<String>, CliError> {
        let mut words = Vec::new();
        while let Some(word) = self.positional() {
            words.push(utf8(word)?);
        }
        Ok((!words.is_empty()).then(|| words.join(" ")))
    }

    /// Fails on anything left over, so a typo is never silently ignored.
    pub(crate) fn finish(&mut self) -> Result<(), CliError> {
        let unread = self.trailing.take().map(|_| OsString::from("--"));
        match self.rest.pop_front().or(unread) {
            None => Ok(()),
            Some(extra) => Err(CliError::Unexpected {
                command: self.command,
                argument: extra,
            }),
        }
    }
}

pub(crate) fn utf8(value: OsString) -> Result<String, CliError> {
    value.into_string().map_err(CliError::InvalidEncoding)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn args(words: &[&str]) -> Args {
        Args::new(words.iter().map(OsString::from), "test")
    }

    #[test]
    fn options_are_found_anywhere_and_values_cannot_be_flags() {
        let mut parsed = args(&["a", "--wait", "5", "--json", "b"]);
        assert!(parsed.flag("--json"));
        assert_eq!(parsed.number("--wait", 1..=10u64).unwrap(), Some(5));
        assert_eq!(parsed.required_text("x").unwrap(), "a");
        assert_eq!(parsed.optional_text().unwrap().as_deref(), Some("b"));
        parsed.finish().unwrap();
        assert_eq!(
            args(&["--workspace", "--json"])
                .option("--workspace")
                .unwrap_err(),
            CliError::MissingValue("--workspace")
        );
        assert_eq!(
            args(&["--wait"]).option("--wait").unwrap_err(),
            CliError::MissingValue("--wait")
        );
        assert_eq!(
            args(&["--wait", "1", "--wait", "2"])
                .option("--wait")
                .unwrap_err(),
            CliError::DuplicateOption("--wait")
        );
        assert_eq!(
            args(&["--wait", "99"])
                .number("--wait", 1..=10u64)
                .unwrap_err(),
            CliError::InvalidValue("--wait")
        );
    }

    #[test]
    fn leftovers_and_missing_arguments_are_errors() {
        assert_eq!(
            args(&["--new"]).finish().unwrap_err(),
            CliError::Unexpected {
                command: "test",
                argument: "--new".into()
            }
        );
        assert_eq!(
            args(&[]).required("FILE").unwrap_err(),
            CliError::MissingArgument {
                command: "test",
                what: "FILE"
            }
        );
        let mut words = args(&["fix", "the", "bug", "--agents", "2"]);
        assert_eq!(words.number("--agents", 1..=8u8).unwrap(), Some(2));
        assert_eq!(words.words().unwrap().as_deref(), Some("fix the bug"));
        // A lone dash is a value, as for stdin.
        assert_eq!(args(&["-"]).required_text("x").unwrap(), "-");
    }

    #[test]
    fn everything_after_a_double_dash_is_left_as_it_is() {
        let mut parsed = args(&["id", "--", "git", "log", "--json", "--"]);
        assert!(!parsed.flag("--json"));
        assert!(!parsed.wants_help());
        assert_eq!(parsed.required_text("ID").unwrap(), "id");
        assert_eq!(
            parsed.trailing().unwrap(),
            ["git", "log", "--json", "--"].map(OsString::from)
        );
        parsed.finish().unwrap();
        // A `--` no command reads is refused, like any other leftover.
        assert!(args(&["--", "x"]).finish().is_err());
        assert_eq!(args(&["a"]).trailing(), None);
    }
}
