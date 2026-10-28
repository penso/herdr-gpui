//! Embeds `locales/<tag>/<feature>.ftl` and checks them (docs/i18n.md):
//! every file parses, every message is prefixed by its feature, and no ID
//! repeats within a language. Writes `$OUT_DIR/messages.rs` with the files
//! and the `en-US` messages with the variables each takes, which `fl!`
//! checks its calls against while compiling.

use fluent_syntax::ast;
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt, fs,
    path::{Path, PathBuf},
};

const FALLBACK: &str = "en-US";

#[derive(Debug)]
pub enum Error {
    Io(PathBuf, std::io::Error),
    NotUtf8(PathBuf),
    Parse(PathBuf, Vec<fluent_syntax::parser::ParserError>),
    Prefix { path: PathBuf, id: String },
    Duplicate { language: String, id: String },
    Missing(PathBuf),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(path, _) => write!(f, "could not read {}", path.display()),
            Self::NotUtf8(path) => write!(f, "{} is not a UTF-8 name", path.display()),
            Self::Parse(path, errors) => {
                write!(f, "{} is not valid Fluent: {errors:?}", path.display())
            }
            Self::Prefix { path, id } => write!(
                f,
                "{id} in {} must start with the file's name and a hyphen",
                path.display()
            ),
            Self::Duplicate { language, id } => write!(f, "{id} is defined twice in {language}"),
            Self::Missing(path) => write!(f, "{} has no {FALLBACK} messages", path.display()),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(_, error) => Some(error),
            _ => None,
        }
    }
}

struct Ftl {
    language: String,
    feature: String,
    path: PathBuf,
}

pub fn generate(manifest: &Path, out: &Path) -> Result<(), Error> {
    let root = manifest.join("locales");
    println!("cargo:rerun-if-changed={}", root.display());
    let mut files = Vec::new();
    for language in sorted_dir(&root)? {
        println!("cargo:rerun-if-changed={}", language.display());
        for path in sorted_dir(&language)? {
            if path.extension().is_none_or(|extension| extension != "ftl") {
                continue;
            }
            println!("cargo:rerun-if-changed={}", path.display());
            files.push(Ftl {
                language: name(&language)?,
                feature: path
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .ok_or_else(|| Error::NotUtf8(path.clone()))?
                    .to_owned(),
                path,
            });
        }
    }
    if !files.iter().any(|file| file.language == FALLBACK) {
        return Err(Error::Missing(root));
    }
    let mut seen = BTreeSet::new();
    // ID -> the variables it takes, and the messages it references.
    let mut fallback: BTreeMap<String, (BTreeSet<String>, BTreeSet<String>)> = BTreeMap::new();
    for file in &files {
        let source =
            fs::read_to_string(&file.path).map_err(|error| Error::Io(file.path.clone(), error))?;
        let resource = fluent_syntax::parser::parse(source.as_str())
            .map_err(|(_, errors)| Error::Parse(file.path.clone(), errors))?;
        for entry in &resource.body {
            let ast::Entry::Message(message) = entry else {
                continue;
            };
            let id = message.id.name.to_owned();
            if !id.starts_with(&format!("{}-", file.feature)) {
                return Err(Error::Prefix {
                    path: file.path.clone(),
                    id,
                });
            }
            if !seen.insert((file.language.clone(), id.clone())) {
                return Err(Error::Duplicate {
                    language: file.language.clone(),
                    id,
                });
            }
            if file.language == FALLBACK {
                let mut uses = (BTreeSet::new(), BTreeSet::new());
                if let Some(pattern) = &message.value {
                    pattern_uses(pattern, &mut uses);
                }
                fallback.insert(id, uses);
            }
        }
    }
    let mut code = String::from("pub(crate) const FILES: &[herdr_i18n::File] = &[\n");
    for file in &files {
        code.push_str(&format!(
            "    herdr_i18n::File {{ language: {:?}, feature: {:?}, source: include_str!({:?}) }},\n",
            file.language,
            file.feature,
            file.path.display().to_string(),
        ));
    }
    code.push_str("];\n\n/// Every `en-US` message, sorted by ID, with the variables it takes.\n");
    code.push_str("pub(crate) const MESSAGES: &[(&str, &[&str])] = &[\n");
    for id in fallback.keys() {
        let variables = variables(id, &fallback, &mut BTreeSet::new());
        let variables: Vec<String> = variables.iter().map(|name| format!("{name:?}")).collect();
        code.push_str(&format!("    ({id:?}, &[{}]),\n", variables.join(", ")));
    }
    code.push_str("];\n");
    let path = out.join("messages.rs");
    fs::write(&path, code).map_err(|error| Error::Io(path, error))
}

fn sorted_dir(path: &Path) -> Result<Vec<PathBuf>, Error> {
    let mut entries = fs::read_dir(path)
        .and_then(|entries| {
            entries
                .map(|entry| entry.map(|entry| entry.path()))
                .collect::<Result<Vec<_>, _>>()
        })
        .map_err(|error| Error::Io(path.to_owned(), error))?;
    entries.sort();
    Ok(entries)
}

fn name(path: &Path) -> Result<String, Error> {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(str::to_owned)
        .ok_or_else(|| Error::NotUtf8(path.to_owned()))
}

/// A message's own variables and those of the messages it references.
fn variables(
    id: &str,
    messages: &BTreeMap<String, (BTreeSet<String>, BTreeSet<String>)>,
    visiting: &mut BTreeSet<String>,
) -> BTreeSet<String> {
    let Some((own, references)) = messages.get(id) else {
        return BTreeSet::new();
    };
    if !visiting.insert(id.to_owned()) {
        return BTreeSet::new();
    }
    let mut all = own.clone();
    for reference in references {
        all.extend(variables(reference, messages, visiting));
    }
    all
}

type Uses = (BTreeSet<String>, BTreeSet<String>);

fn pattern_uses(pattern: &ast::Pattern<&str>, uses: &mut Uses) {
    for element in &pattern.elements {
        if let ast::PatternElement::Placeable { expression } = element {
            expression_uses(expression, uses);
        }
    }
}

fn expression_uses(expression: &ast::Expression<&str>, uses: &mut Uses) {
    match expression {
        ast::Expression::Select { selector, variants } => {
            inline_uses(selector, uses);
            for variant in variants {
                pattern_uses(&variant.value, uses);
            }
        }
        ast::Expression::Inline(inline) => inline_uses(inline, uses),
    }
}

fn inline_uses(inline: &ast::InlineExpression<&str>, uses: &mut Uses) {
    match inline {
        ast::InlineExpression::VariableReference { id } => {
            uses.0.insert(id.name.to_owned());
        }
        ast::InlineExpression::MessageReference { id, .. } => {
            uses.1.insert(id.name.to_owned());
        }
        ast::InlineExpression::FunctionReference { arguments, .. } => {
            for argument in &arguments.positional {
                inline_uses(argument, uses);
            }
            for argument in &arguments.named {
                inline_uses(&argument.value, uses);
            }
        }
        ast::InlineExpression::Placeable { expression } => expression_uses(expression, uses),
        // A term's arguments are its own; it takes nothing from the caller.
        ast::InlineExpression::TermReference { .. }
        | ast::InlineExpression::StringLiteral { .. }
        | ast::InlineExpression::NumberLiteral { .. } => {}
    }
}
