//! Values a script can pass along but never read: the firing's date and
//! time, and what earlier steps produced. The script sees fixed tokens such
//! as `{date}` or `{log_tail:2}`, so its control flow cannot depend on them
//! and the plan shows every path it can take; the runner fills the tokens
//! in only when it hands a step to the host.
//!
//! Step output is untrusted, so it never becomes shell text: in a `sh`
//! command an output token becomes a quoted environment variable reference
//! and the value travels in the environment. The reference is quoted for
//! where the token sits, so quotes the script already put around it do not
//! leave it open to word splitting or globbing.

use crate::step::{Call, Step};

pub const DATE: &str = "{date}";
pub const TIME: &str = "{time}";

/// The token for step `index`'s output tail; steps count from 1 for people.
pub fn log_tail(index: usize) -> String {
    format!("{{log_tail:{}}}", index + 1)
}

/// The token for the agent run step `index` started.
pub fn run_id(index: usize) -> String {
    format!("{{run_id:{}}}", index + 1)
}

/// The real values behind the tokens, for one run.
#[derive(Clone, Copy, Debug)]
pub struct Values<'a> {
    pub date: &'a str,
    pub time: &'a str,
    /// Indexed by step; a missing or `None` entry leaves its token as is.
    pub log_tails: &'a [Option<String>],
    pub run_ids: &'a [Option<String>],
}

impl Values<'_> {
    /// `text` with every known token replaced. Unknown braces stay as they
    /// are, so a script cannot break a step by writing `{` itself.
    pub fn fill(&self, text: &str) -> String {
        self.replace(text)
    }

    /// A POSIX shell command with each output token turned into a
    /// reference to a variable, quoted for where it sits: `"$NAME"` bare,
    /// `${NAME}` inside double quotes, and `'"$NAME"'` inside single quotes,
    /// which closes and reopens them. Returns the variables to set. Date and
    /// time are digits, dashes and colons, so they are written in directly.
    pub fn fill_shell(&self, text: &str) -> (String, Vec<(String, String)>) {
        let mut env = Vec::new();
        let mut filled = String::with_capacity(text.len());
        let mut quoting = Quoting::default();
        let mut rest = text;
        while let Some(c) = rest.chars().next() {
            if c == '{'
                && let Some(close) = rest.find('}')
                && let token = &rest[..=close]
                && let Some(value) = self.value(token)
            {
                if token == DATE || token == TIME {
                    filled.push_str(value);
                } else {
                    let name = variable(token);
                    filled.push_str(&quoting.reference(&name));
                    if !env.iter().any(|(known, _)| *known == name) {
                        env.push((name, value.to_owned()));
                    }
                }
                rest = &rest[close + 1..];
                continue;
            }
            let step = quoting.read(rest);
            filled.push_str(&rest[..step]);
            rest = &rest[step..];
        }
        (filled, env)
    }

    fn replace(&self, text: &str) -> String {
        let mut filled = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(open) = rest.find('{') {
            filled.push_str(&rest[..open]);
            let tail = &rest[open..];
            let Some(close) = tail.find('}') else {
                filled.push_str(tail);
                return filled;
            };
            let token = &tail[..=close];
            filled.push_str(self.value(token).unwrap_or(token));
            rest = &tail[close + 1..];
        }
        filled.push_str(rest);
        filled
    }

    fn value(&self, token: &str) -> Option<&str> {
        match token {
            DATE => return Some(self.date),
            TIME => return Some(self.time),
            _ => {}
        }
        let inner = token.strip_prefix('{')?.strip_suffix('}')?;
        let (name, number) = inner.split_once(':')?;
        let index = number.parse::<usize>().ok()?.checked_sub(1)?;
        let list = match name {
            "log_tail" => self.log_tails,
            "run_id" => self.run_ids,
            _ => return None,
        };
        list.get(index)?.as_deref()
    }
}

/// Whether every output token in a shell command sits where [`Quoting`]
/// can tell its quoting. It models quotes, backslashes, `$(…)` and
/// backticks; a command that uses an earlier step's output must not hold
/// what it does not model (comments, heredocs, `$'…'`, `$"…"`, `${…}`),
/// and must close every quote, or a reference could land unquoted.
pub(crate) fn shell_tokens_placeable(command: &str) -> bool {
    if !command.contains("{log_tail:") && !command.contains("{run_id:") {
        return true;
    }
    if ["#", "<<", "$'", "$\"", "${"]
        .iter()
        .any(|unmodelled| command.contains(unmodelled))
    {
        return false;
    }
    let mut quoting = Quoting::default();
    let mut rest = command;
    while !rest.is_empty() {
        let step = quoting.read(rest);
        rest = &rest[step..];
    }
    !quoting.single && quoting.open.is_empty()
}

/// Where a POSIX shell reader is: which quotes are open, inside how many
/// command substitutions, each of which starts its own quoting.
#[derive(Debug, Default)]
struct Quoting {
    /// Open contexts, innermost last; empty is bare text.
    open: Vec<Context>,
    single: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Context {
    Double,
    /// `$(`, or a `(` inside one.
    Paren,
    Backtick,
}

impl Quoting {
    /// A variable reference that expands to exactly its value here.
    fn reference(&self, name: &str) -> String {
        if self.single {
            format!("'\"${name}\"'")
        } else if self.open.last() == Some(&Context::Double) {
            format!("${{{name}}}")
        } else {
            format!("\"${name}\"")
        }
    }

    /// Reads the next character or escape of `text`; returns its length.
    fn read(&mut self, text: &str) -> usize {
        let mut chars = text.chars();
        let Some(c) = chars.next() else {
            return 0;
        };
        let len = c.len_utf8();
        if self.single {
            self.single = c != '\'';
            return len;
        }
        let top = self.open.last().copied();
        match c {
            // An escaped character is taken as it is, so `\{` is no token.
            '\\' => len + chars.next().map_or(0, char::len_utf8),
            '"' if top == Some(Context::Double) => {
                self.open.pop();
                len
            }
            '"' => {
                self.open.push(Context::Double);
                len
            }
            '\'' if top != Some(Context::Double) => {
                self.single = true;
                len
            }
            '$' if text[len..].starts_with('(') => {
                self.open.push(Context::Paren);
                len + 1
            }
            '(' if top == Some(Context::Paren) => {
                self.open.push(Context::Paren);
                len
            }
            ')' if top == Some(Context::Paren) => {
                self.open.pop();
                len
            }
            '`' if top == Some(Context::Backtick) => {
                self.open.pop();
                len
            }
            '`' => {
                self.open.push(Context::Backtick);
                len
            }
            _ => len,
        }
    }
}

/// `{log_tail:2}` as `HERDR_LOG_TAIL_2`.
fn variable(token: &str) -> String {
    let inner = token.trim_start_matches('{').trim_end_matches('}');
    format!("HERDR_{}", inner.replace(':', "_").to_ascii_uppercase())
}

/// A call with its tokens filled in, ready to hand to the host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resolved {
    pub call: Call,
    /// Variables a `sh` command reads its output tokens from.
    pub env: Vec<(String, String)>,
}

impl Call {
    pub fn resolve(&self, values: &Values<'_>) -> Resolved {
        let mut env = Vec::new();
        let fill = |text: &String| values.fill(text);
        let fill_opt = |text: &Option<String>| text.as_ref().map(fill);
        let step = match &self.step {
            Step::Agent {
                harness,
                prompt,
                worktree,
                base,
                wait,
            } => Step::Agent {
                harness: fill(harness),
                prompt: fill(prompt),
                worktree: fill_opt(worktree),
                base: fill_opt(base),
                wait: *wait,
            },
            Step::Notify { text } => Step::Notify { text: fill(text) },
            Step::Pull { branch } => Step::Pull {
                branch: fill(branch),
            },
            Step::Run { name } => Step::Run { name: name.clone() },
            Step::Sh { command } => {
                let (command, variables) = values.fill_shell(command);
                env = variables;
                Step::Sh { command }
            }
            Step::Worktree { name, base } => Step::Worktree {
                name: fill(name),
                base: fill_opt(base),
            },
        };
        Resolved {
            call: Self {
                step,
                check: self.check,
                timeout: self.timeout,
            },
            env,
        }
    }
}
