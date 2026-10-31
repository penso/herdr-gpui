//! Presentation-only patch rows. Source bytes remain the selection/copy payload;
//! paths are grammar hints, never filesystem requests. Call only for diff/patch.

const MAX_ROWS: usize = 512;
const MAX_HEADER_BYTES: usize = 1_024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Context,
    Added,
    Removed,
    Meta,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Row<'a> {
    pub(crate) kind: Kind,
    /// One source byte for a recognized content marker; zero for metadata or
    /// unrecognized text. It is a grammar offset, not permission to alter copy.
    pub(crate) prefix: usize,
    pub(crate) path: Option<&'a str>,
}

#[derive(Default)]
enum Mode {
    #[default]
    Outside,
    Counted {
        old: u32,
        new: u32,
    },
    Fragment,
    AddFile,
}

#[derive(Default)]
struct Parser<'a> {
    mode: Mode,
    apply: bool,
    old: Option<&'a str>,
    new: Option<&'a str>,
}

/// Inspect at most the grammar budget and the caller's displayed line prefix.
/// Rows beyond either bound are unclassified; callers leave them uncoloured.
pub(crate) fn rows(source: &str, limit: usize) -> Vec<Row<'_>> {
    let mut end = source.len().min(super::MAX_CODE_BYTES);
    while !source.is_char_boundary(end) {
        end -= 1;
    }
    let prefix = &source[..end];
    let native = (limit > 1).then(|| initial_path(prefix)).flatten();
    let mut parser = Parser {
        old: native,
        new: native,
        ..Parser::default()
    };
    prefix
        .split_inclusive('\n')
        .take(limit.min(MAX_ROWS))
        .enumerate()
        .map(|(index, line)| {
            if index == 0 && native.is_some() {
                return parser.meta();
            }
            if end < source.len() && !line.ends_with('\n') {
                return unknown();
            }
            let line = line.strip_suffix('\n').unwrap_or(line);
            parser.line(line.strip_suffix('\r').unwrap_or(line))
        })
        .collect()
}

/// Native Codex supplies a filename before its first numbered hunk. This
/// does not recognize arbitrary prose unless that next line is a valid hunk.
fn initial_path(source: &str) -> Option<&str> {
    let (first, rest) = source.split_once('\n')?;
    let first = first.strip_suffix('\r').unwrap_or(first);
    if first.len() > MAX_HEADER_BYTES
        || first.chars().any(char::is_control)
        || ["--- ", "+++ ", "diff --git ", "*** ", "@@"]
            .iter()
            .any(|marker| first.starts_with(marker))
    {
        return None;
    }
    let next = rest.split('\n').next()?;
    let (old, new) = counts(next.strip_suffix('\r').unwrap_or(next))?;
    (old > 0 || new > 0).then(|| path(first)).flatten()
}

fn unknown() -> Row<'static> {
    Row {
        kind: Kind::Context,
        prefix: 0,
        path: None,
    }
}

impl<'a> Parser<'a> {
    fn meta(&self) -> Row<'a> {
        Row {
            kind: Kind::Meta,
            prefix: 0,
            path: self.new.or(self.old),
        }
    }

    fn line(&mut self, line: &'a str) -> Row<'a> {
        if line == "*** Begin Patch" || line == "*** End Patch" || line.starts_with("diff --git ") {
            *self = Self {
                apply: line == "*** Begin Patch",
                ..Self::default()
            };
            return self.meta();
        }
        if line.starts_with("*** ") {
            self.mode = Mode::Outside;
            if self.apply {
                if let Some(name) = line.strip_prefix("*** Add File: ") {
                    self.old = None;
                    self.new = path(name);
                    if self.new.is_some() {
                        self.mode = Mode::AddFile;
                    }
                } else if let Some(name) = line.strip_prefix("*** Update File: ") {
                    self.old = path(name);
                    self.new = self.old;
                    if self.new.is_some() {
                        self.mode = Mode::Fragment;
                    }
                } else if let Some(name) = line.strip_prefix("*** Delete File: ") {
                    self.old = path(name);
                    self.new = None;
                } else if let Some(name) = line.strip_prefix("*** Move to: ") {
                    self.new = path(name);
                    if self.old.is_some() && self.new.is_some() {
                        self.mode = Mode::Fragment;
                    }
                }
            }
            return self.meta();
        }
        if line.starts_with("@@") {
            self.mode = if line == "@@ Provided text @@"
                || (self.apply && self.new.is_some() && (line == "@@" || line.starts_with("@@ ")))
            {
                Mode::Fragment
            } else if let Some((old, new)) = counts(line) {
                Mode::Counted { old, new }
            } else {
                Mode::Outside
            };
            return self.meta();
        }
        let first = line.as_bytes().first().copied();
        let kind = match (&mut self.mode, first) {
            (Mode::Counted { old, new }, Some(b' ')) if *old > 0 && *new > 0 => {
                *old -= 1;
                *new -= 1;
                Some(Kind::Context)
            }
            (Mode::Counted { new, .. }, Some(b'+')) if *new > 0 => {
                *new -= 1;
                Some(Kind::Added)
            }
            (Mode::Counted { old, .. }, Some(b'-')) if *old > 0 => {
                *old -= 1;
                Some(Kind::Removed)
            }
            (Mode::Fragment, Some(b' ')) => Some(Kind::Context),
            (Mode::Fragment | Mode::AddFile, Some(b'+')) => Some(Kind::Added),
            (Mode::Fragment, Some(b'-')) => Some(Kind::Removed),
            _ => None,
        };
        if let Some(kind) = kind {
            let path = if kind == Kind::Removed {
                self.old.or(self.new)
            } else {
                self.new.or(self.old)
            };
            if matches!(self.mode, Mode::Counted { old: 0, new: 0 }) {
                self.mode = Mode::Outside;
            }
            return Row {
                kind,
                prefix: 1,
                path,
            };
        }
        if line.starts_with("\\ ") {
            return self.meta();
        }
        self.mode = Mode::Outside;
        if let Some(name) = line.strip_prefix("--- ") {
            self.old = path(name);
            self.new = None;
            return self.meta();
        }
        if let Some(name) = line.strip_prefix("+++ ") {
            self.new = path(name);
            return self.meta();
        }
        unknown()
    }
}

fn path(header: &str) -> Option<&str> {
    if header.len() > MAX_HEADER_BYTES {
        return None;
    }
    let name = header.split('\t').next()?;
    let name = if let Some(quoted) = name.strip_prefix('"') {
        quoted.strip_suffix('"')?
    } else {
        name
    };
    (!name.is_empty() && name != "/dev/null").then_some(name)
}

fn counts(header: &str) -> Option<(u32, u32)> {
    if header.len() > MAX_HEADER_BYTES {
        return None;
    }
    let mut parts = header.strip_prefix("@@ ")?.split(' ');
    let old = count(parts.next()?, '-')?;
    let new = count(parts.next()?, '+')?;
    (parts.next()? == "@@").then_some((old, new))
}

fn count(range: &str, sign: char) -> Option<u32> {
    let range = range.strip_prefix(sign)?;
    let (start, count) = range.split_once(',').unwrap_or((range, "1"));
    if !start.bytes().all(|byte| byte.is_ascii_digit())
        || !count.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    start.parse::<u32>().ok()?;
    count.parse().ok()
}

#[cfg(test)]
mod tests;
