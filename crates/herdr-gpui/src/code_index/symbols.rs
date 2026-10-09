//! Definitions read from source lines by their leading keywords, the way
//! `ctags` finds them, for jumping around a checkout. This is a heuristic,
//! not a parser: a definition split oddly over lines, or one a macro makes,
//! is missed, and a keyword at the start of a string line can be read as
//! one. Each line is looked at once, so a file is scanned in linear time.

/// What a definition defines, as the list shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Kind {
    Function,
    /// A function indented under another definition.
    Method,
    /// A struct, class, enum, trait, interface, protocol, or alias.
    Type,
    /// A module or namespace.
    Module,
    Constant,
    /// An `impl` or `extension` block.
    Implementation,
    Macro,
}

impl Kind {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Function => "function",
            Self::Method => "method",
            Self::Type => "type",
            Self::Module => "module",
            Self::Constant => "constant",
            Self::Implementation => "impl",
            Self::Macro => "macro",
        }
    }
}

/// The languages whose definitions are read, by file extension.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Language {
    Rust,
    Python,
    Ruby,
    Swift,
    Go,
    Script,
    Kotlin,
    Java,
    Elixir,
    Zig,
    C,
}

impl Language {
    pub(crate) fn of(path: &str) -> Option<Self> {
        let extension = path.rsplit_once('.')?.1;
        Some(match extension {
            "rs" => Self::Rust,
            "py" | "pyi" => Self::Python,
            "rb" | "rake" => Self::Ruby,
            "swift" => Self::Swift,
            "go" => Self::Go,
            "js" | "jsx" | "mjs" | "cjs" | "ts" | "tsx" | "mts" | "cts" => Self::Script,
            "kt" | "kts" => Self::Kotlin,
            "java" | "cs" | "scala" => Self::Java,
            "ex" | "exs" => Self::Elixir,
            "zig" => Self::Zig,
            "c" | "h" | "cc" | "cpp" | "cxx" | "hpp" | "hh" | "m" | "mm" => Self::C,
            _ => return None,
        })
    }

    /// Words that may precede a definition's keyword without changing it.
    fn modifiers(self) -> &'static [&'static str] {
        match self {
            Self::Rust => &["pub", "async", "unsafe", "extern", "default", "\"C\""],
            Self::Python => &["async"],
            Self::Ruby | Self::Elixir | Self::C => &[],
            Self::Swift => &[
                "public",
                "private",
                "fileprivate",
                "internal",
                "open",
                "static",
                "final",
                "override",
                "mutating",
                "nonisolated",
                "indirect",
                "convenience",
                "required",
                "dynamic",
                "@MainActor",
                "@objc",
                "@inlinable",
                "@discardableResult",
            ],
            Self::Go => &[],
            Self::Script => &["export", "default", "async", "declare", "abstract"],
            Self::Kotlin => &[
                "public",
                "private",
                "protected",
                "internal",
                "open",
                "override",
                "abstract",
                "sealed",
                "data",
                "inline",
                "value",
                "suspend",
                "enum",
                "annotation",
                "companion",
                "inner",
            ],
            Self::Java => &[
                "public",
                "private",
                "protected",
                "internal",
                "static",
                "final",
                "abstract",
                "sealed",
                "partial",
                "readonly",
                "case",
            ],
            Self::Zig => &["pub", "export", "inline"],
        }
    }

    /// The keywords that open a definition, and what each defines.
    fn keywords(self) -> &'static [(&'static str, Kind)] {
        use Kind::*;
        match self {
            Self::Rust => &[
                ("fn", Function),
                ("struct", Type),
                ("enum", Type),
                ("trait", Type),
                ("type", Type),
                ("union", Type),
                ("mod", Module),
                ("const", Constant),
                ("static", Constant),
                ("impl", Implementation),
                ("macro_rules!", Macro),
            ],
            Self::Python => &[("def", Function), ("class", Type)],
            Self::Ruby => &[("def", Function), ("class", Type), ("module", Module)],
            Self::Swift => &[
                ("func", Function),
                ("init", Function),
                ("class", Type),
                ("struct", Type),
                ("enum", Type),
                ("protocol", Type),
                ("actor", Type),
                ("typealias", Type),
                ("extension", Implementation),
            ],
            Self::Go => &[("func", Function), ("type", Type)],
            Self::Script => &[
                ("function", Function),
                ("function*", Function),
                ("class", Type),
                ("interface", Type),
                ("type", Type),
                ("enum", Type),
                ("namespace", Module),
                ("const", Constant),
            ],
            Self::Kotlin => &[
                ("fun", Function),
                ("class", Type),
                ("interface", Type),
                ("object", Type),
                ("typealias", Type),
            ],
            Self::Java => &[
                ("class", Type),
                ("interface", Type),
                ("enum", Type),
                ("record", Type),
                ("struct", Type),
                ("namespace", Module),
            ],
            Self::Elixir => &[
                ("def", Function),
                ("defp", Function),
                ("defmacro", Macro),
                ("defmodule", Module),
            ],
            Self::Zig => &[("fn", Function), ("const", Constant)],
            Self::C => &[
                ("struct", Type),
                ("enum", Type),
                ("union", Type),
                ("class", Type),
                ("namespace", Module),
                ("#define", Macro),
                ("@interface", Type),
                ("@implementation", Implementation),
            ],
        }
    }
}

/// One definition: its name, what it is, and its 1-based line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Definition {
    pub(crate) name: String,
    pub(crate) kind: Kind,
    pub(crate) line: u32,
}

/// The longest name kept, so a minified line cannot fill the index.
const MAX_NAME: usize = 120;

fn identifier<'a>(text: &'a str, extra: &[char]) -> Option<&'a str> {
    let end = text
        .char_indices()
        .find(|(_, c)| !(c.is_alphanumeric() || *c == '_' || extra.contains(c)))
        .map_or(text.len(), |(index, _)| index);
    let name = &text[..end];
    (!name.is_empty() && !name.starts_with(|c: char| c.is_ascii_digit())).then_some(name)
}

/// `text` after `word` and the whitespace following it, when it starts with
/// that whole word.
fn after<'a>(text: &'a str, word: &str) -> Option<&'a str> {
    let rest = text.strip_prefix(word)?;
    let trimmed = rest.trim_start();
    // `pub(crate)` and `@objc(name)` take their parenthesised argument, and
    // `impl<T>` its parameters.
    if trimmed.len() == rest.len() && !rest.starts_with(['(', '<']) {
        return None;
    }
    Some(trimmed)
}

fn skip_parens(text: &str) -> &str {
    let Some(rest) = text.strip_prefix('(') else {
        return text;
    };
    rest.find(')')
        .map_or(text, |close| rest[close + 1..].trim_start())
}

/// `text` without leading generic parameters such as `<T: Clone>`.
fn skip_generics(text: &str) -> &str {
    if !text.starts_with('<') {
        return text;
    }
    let mut depth = 0usize;
    for (index, c) in text.char_indices() {
        match c {
            '<' => depth += 1,
            '>' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return text[index + 1..].trim_start();
                }
            }
            _ => {}
        }
    }
    text
}

/// `text` without a word that is a modifier only before certain others,
/// such as Rust's `const fn` or Swift's `class func`.
fn soft_modifier(language: Language, text: &str) -> Option<&str> {
    let (word, before): (&str, &[&str]) = match language {
        Language::Rust => ("const", &["fn", "unsafe", "async", "extern"]),
        Language::Swift => ("class", &["func", "var", "let", "subscript", "override"]),
        _ => return None,
    };
    let rest = after(text, word)?;
    before
        .iter()
        .any(|next| after(rest, next).is_some())
        .then_some(rest)
}

/// The definition one line opens, if any.
fn definition(language: Language, line: &str) -> Option<(String, Kind)> {
    let mut text = line.trim_start();
    'modifiers: loop {
        for modifier in language.modifiers() {
            if let Some(rest) = after(text, modifier) {
                text = skip_parens(rest);
                continue 'modifiers;
            }
        }
        if let Some(rest) = soft_modifier(language, text) {
            text = rest;
            continue;
        }
        break;
    }
    let extra: &[char] = match language {
        Language::Ruby | Language::Elixir => &['?', '!'],
        Language::Script => &['$'],
        _ => &[],
    };
    for &(keyword, kind) in language.keywords() {
        let Some(mut rest) = after(text, keyword) else {
            continue;
        };
        if kind == Kind::Implementation {
            // `impl<T> Display for Wrapper<T>` reads as written, up to its body.
            let head = skip_generics(rest)
                .split(['{', '\n'])
                .next()
                .unwrap_or_default()
                .trim();
            let head = head.strip_suffix("where").unwrap_or(head).trim();
            if head.is_empty() {
                return None;
            }
            // The head is file text, shown as is: keep only safe characters.
            let head: String = head
                .chars()
                .filter(|c| !crate::notifications::unsafe_char(*c))
                .collect();
            let name = format!("{keyword} {head}");
            return Some((name.chars().take(MAX_NAME).collect(), kind));
        }
        match (language, keyword) {
            // A Go method names its receiver first: `func (s *Server) Run(`.
            (Language::Go, "func") => rest = skip_parens(rest),
            // `def self.call` is a class method.
            (Language::Ruby, "def") => rest = rest.strip_prefix("self.").unwrap_or(rest),
            (Language::Swift, "init") => return Some(("init".into(), Kind::Function)),
            _ => {}
        }
        let name = identifier(rest, extra)?;
        // A script constant counts only when it holds a function.
        if (language, keyword) == (Language::Script, "const") {
            let value = rest[name.len()..]
                .trim_start()
                .strip_prefix('=')?
                .trim_start();
            let value = value.strip_prefix("async").unwrap_or(value).trim_start();
            if !(value.starts_with('(') || value.starts_with("function")) {
                return None;
            }
            return Some((name.into(), Kind::Function));
        }
        // A Zig constant counts only when it names a type or an import.
        if (language, keyword) == (Language::Zig, "const") {
            let value = rest[name.len()..]
                .trim_start()
                .strip_prefix('=')?
                .trim_start();
            let kind = ["struct", "enum", "union", "opaque"]
                .iter()
                .any(|word| value.starts_with(word))
                .then_some(Kind::Type)?;
            return Some((name.into(), kind));
        }
        let kind = match kind {
            Kind::Function if line.starts_with([' ', '\t']) => Kind::Method,
            kind => kind,
        };
        return Some((name.chars().take(MAX_NAME).collect(), kind));
    }
    None
}

/// The definitions in `text`, at most `limit` of them.
pub(crate) fn scan(language: Language, text: &str, limit: usize) -> Vec<Definition> {
    let mut found = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if found.len() >= limit {
            break;
        }
        // Long lines are generated or minified, never hand-written definitions.
        if line.len() > 1000 {
            continue;
        }
        if let Some((name, kind)) = definition(language, line) {
            found.push(Definition {
                name,
                kind,
                line: u32::try_from(index + 1).unwrap_or(u32::MAX),
            });
        }
    }
    found
}
