#!/usr/bin/env python3
"""Check the app's Fluent messages (docs/i18n.md).

Every `crates/*/locales/<tag>/<feature>.ftl` must parse as Fluent's message
syntax, and each message ID start with its file's feature and a hyphen.
Within a language no ID repeats, and a translation has no ID English lacks.
Every English message is used: the app's Rust code names it as a string
literal (`fl!("id")`, or a table such as the command catalog's), or another
message references it. Tests do not count, so a message only a test names is
reported. Every `fl!` names an English message; the compiler checks
that too, and the build script parses each file in full.

A module whose code shows only messages carries the line `// i18n: migrated`.
That file, and every file in the directory of the same name, may then hold
no string literal that reads as English (a capitalized word, or two words
with a space between), except where it is not shown: logs, panics,
attributes such as `#[error]`, `write!` in a `Display` for logs, key
contexts and bindings. A literal that is data, such as a font or a theme
name, takes `// i18n: data` at the end of its line or on a line of its own
just above it.

    python3 scripts/check-i18n.py
"""

from __future__ import annotations

import re
import subprocess
import sys
from dataclasses import dataclass, field
from pathlib import Path

FALLBACK = "en-US"
IDENTIFIER = r"[a-zA-Z][a-zA-Z0-9_-]*"
ENTRY = re.compile(rf"^(-?{IDENTIFIER})\s*=(.*)$")
ATTRIBUTE = re.compile(rf"^\s+\.{IDENTIFIER}\s*=")
REFERENCE = re.compile(rf"\{{\s*({IDENTIFIER})(?:\.{IDENTIFIER})?\s*\}}")
FL = re.compile(r"\bfl!\(\s*\"([^\"]*)\"")
LITERAL = re.compile(r"\"([a-z][a-z0-9-]*)\"")
MIGRATED = "// i18n: migrated"
DATA = "// i18n: data"
# A capitalized word, or two lowercase words with a space between, outside
# format placeholders.
ENGLISH = re.compile(r"\b[A-Z][a-z]+|\b[a-z]{2,} [a-z]{2,}\b")
PLACEHOLDER = re.compile(r"\{[^{}]*\}")
# Calls whose string arguments are never shown, by the name before their
# opening bracket. `#` is an attribute.
NOT_SHOWN = {
    "#",
    "actions!",
    "anyhow!",
    "assert!",
    "assert_eq!",
    "assert_ne!",
    "bail!",
    "concat!",
    "context",
    "debug!",
    "debug_assert!",
    "debug_assert_eq!",
    "debug_assert_ne!",
    "debug_selector",
    "ensure!",
    "env!",
    "error!",
    "expect",
    "include_str!",
    "info!",
    "key_context",
    "KeyBinding::new",
    "panic!",
    "trace!",
    "todo!",
    "unreachable!",
    "warn!",
    "with_context",
    "write!",
    "writeln!",
}


@dataclass
class Ftl:
    path: Path
    language: str
    feature: str
    # ID -> line, in file order.
    messages: dict[str, int] = field(default_factory=dict)
    references: set[str] = field(default_factory=set)


def placeables(text: str, stack: list[str]) -> bool:
    """Reads `text` into `stack`, the placeables open around it, innermost
    last: an `expression`, or a `select` once its `->` is read. Returns
    whether a `}` closed none. Only an expression has string literals, so
    `{ "{" }` holds text; a select's branches are text, where a quote is a
    quote, as it is outside any placeable."""
    unbalanced = False
    quoted = False
    escaped = False
    for index, ch in enumerate(text):
        if quoted:
            if escaped:
                escaped = False
            elif ch == "\\":
                escaped = True
            elif ch == '"':
                quoted = False
        elif ch == "{":
            stack.append("expression")
        elif ch == "}":
            if stack:
                stack.pop()
            else:
                unbalanced = True
        elif stack and stack[-1] == "expression":
            if ch == '"':
                quoted = True
            elif ch == "-" and text[index + 1 : index + 2] == ">":
                stack[-1] = "select"
    return unbalanced


@dataclass
class Literal:
    line: int
    text: str
    # The calls and brackets around it, innermost last.
    around: list[str]


# A name or path. `\w` is Unicode-aware, as Rust identifiers are.
WORD = re.compile(r"\w+(?:::\w+)*!?")


def literals(source: str) -> list[Literal]:
    """The string literals in Rust `source`, skipping comments and chars."""
    found: list[Literal] = []
    stack: list[str] = []
    name = ""
    i, line, n = 0, 1, len(source)
    while i < n:
        ch = source[i]
        if source.startswith("//", i):
            end = source.find("\n", i)
            i = n if end < 0 else end
            continue
        if source.startswith("/*", i):
            depth, i = 1, i + 2
            while i < n and depth:
                if source.startswith("/*", i):
                    depth, i = depth + 1, i + 2
                elif source.startswith("*/", i):
                    depth, i = depth - 1, i + 2
                else:
                    line += source[i] == "\n"
                    i += 1
            continue
        raw = re.match(r'b?r(#*)"', source[i:])
        if raw and (i == 0 or not (source[i - 1].isalnum() or source[i - 1] == "_")):
            close = '"' + raw.group(1)
            start = i + raw.end()
            end = source.find(close, start)
            end = n if end < 0 else end
            text = source[start:end]
            found.append(Literal(line, text, list(stack)))
            line += text.count("\n")
            i, name = end + len(close), ""
            continue
        if ch == '"':
            j, start = i + 1, line
            while j < n and source[j] != '"':
                if source[j] == "\\":
                    j += 1
                line += source[j] == "\n"
                j += 1
            found.append(Literal(start, source[i + 1 : j], list(stack)))
            i, name = j + 1, ""
            continue
        if ch == "'":
            char = re.match(r"'(\\.[^']*|[^\\'])'", source[i:])
            i += char.end() if char else 1
            name = ""
            continue
        word = WORD.match(source, i) if ch.isalnum() or ch == "_" else None
        if word:
            name = word.group(0)
            i = word.end()
            continue
        if ch in "([{":
            stack.append("#" if ch == "[" and source[i - 1 : i] == "#" else name)
        elif ch in ")]}":
            if stack:
                stack.pop()
        elif ch == "!" and name and not name.endswith("!"):
            name += "!"
            i += 1
            continue
        line += ch == "\n"
        if not ch.isspace():
            name = ""
        i += 1
    return found


def shown_english(path: Path, source: str) -> list[str]:
    """Literals in a migrated file that read as English and may be shown."""
    lines = source.splitlines()
    errors: list[str] = []
    for literal in literals(source):
        if not ENGLISH.search(PLACEHOLDER.sub("", literal.text.replace("{{", "").replace("}}", ""))):
            continue
        if any(call.split("::")[-1] in NOT_SHOWN or call in NOT_SHOWN for call in literal.around):
            continue
        here = lines[literal.line - 1] if literal.line <= len(lines) else ""
        above = lines[literal.line - 2].strip() if literal.line >= 2 else ""
        if here.rstrip().endswith(DATA) or above == DATA:
            continue
        first = literal.text.splitlines()[0] if literal.text else ""
        errors.append(f"{path}:{literal.line}: \"{first}\" is not a message")
    return errors


def migrated(sources: dict[Path, str]) -> set[Path]:
    """Files a `// i18n: migrated` marker covers: its own, and those in the
    directory of the same name."""
    roots = [
        path
        for path, text in sources.items()
        if any(line.strip() == MIGRATED for line in text.splitlines())
    ]
    covered = set(roots)
    for root in roots:
        directory = root.with_suffix("")
        covered.update(path for path in sources if directory in path.parents)
    return {path for path in covered if not is_test(path)}


def is_test(path: Path) -> bool:
    """Test files, by the repository's layout: `tests.rs`, a `tests/`
    directory, or a `*_tests` module or directory."""
    return any(
        part == "tests" or part == "tests.rs" or part.removesuffix(".rs").endswith("_tests")
        for part in path.parts
    )


def parse(path: Path, text: str) -> tuple[Ftl, list[str]]:
    """The file's messages and the problems in it, each as `path:line: why`."""
    ftl = Ftl(path, path.parent.name, path.stem)
    errors: list[str] = []
    stack: list[str] = []
    in_entry = False
    for number, line in enumerate(text.splitlines(), start=1):
        stripped = line.strip()
        if not stripped or (line.startswith("#") and not stack):
            # A blank line ends no message: values may span them.
            in_entry = in_entry and not line.startswith("#")
            continue
        entry = ENTRY.match(line) if not stack else None
        if entry:
            in_entry = True
            name = entry.group(1)
            if not name.startswith("-"):
                if name in ftl.messages:
                    errors.append(f"{path}:{number}: {name} is defined twice")
                if not name.startswith(f"{ftl.feature}-"):
                    errors.append(
                        f"{path}:{number}: {name} must start with {ftl.feature}-, the file's name"
                    )
                ftl.messages.setdefault(name, number)
            body = entry.group(2)
        elif in_entry and (line[0] in " \t" or stack):
            body = line
            if ATTRIBUTE.match(line) and not stack:
                body = line.split("=", 1)[1]
        else:
            errors.append(f"{path}:{number}: not a message, comment, or continuation")
            continue
        ftl.references.update(REFERENCE.findall(body))
        if placeables(body, stack):
            errors.append(f"{path}:{number}: unbalanced }}")
    if stack:
        errors.append(f"{path}: a placeable is not closed")
    return ftl, errors


def check(files: list[Ftl], sources: dict[Path, str]) -> list[str]:
    errors: list[str] = []
    by_language: dict[str, dict[str, Path]] = {}
    for ftl in files:
        seen = by_language.setdefault(ftl.language, {})
        for name, line in ftl.messages.items():
            if name in seen and seen[name] != ftl.path:
                errors.append(f"{ftl.path}:{line}: {name} is also defined in {seen[name]}")
            seen.setdefault(name, ftl.path)
    english = by_language.get(FALLBACK, {})
    if files and not english:
        errors.append(f"no {FALLBACK} messages")
    for language, messages in sorted(by_language.items()):
        if language == FALLBACK:
            continue
        for name in sorted(set(messages) - set(english)):
            errors.append(f"{messages[name]}: {name} has no {FALLBACK} message")
    used = {name for ftl in files for name in ftl.references}
    for path, text in sources.items():
        # Comments name messages by way of example.
        text = "\n".join(line for line in text.splitlines() if not line.lstrip().startswith("//"))
        if not is_test(path):
            used.update(LITERAL.findall(text))
        for name in FL.findall(text):
            if name not in english:
                errors.append(f"{path}: fl!(\"{name}\") names no {FALLBACK} message")
    for path in sorted(migrated(sources)):
        errors.extend(shown_english(path, sources[path]))
    for ftl in files:
        if ftl.language != FALLBACK:
            continue
        for name, line in ftl.messages.items():
            if name not in used:
                errors.append(f"{ftl.path}:{line}: {name} is not used")
    return errors


def tracked(*patterns: str) -> list[Path]:
    output = subprocess.run(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard", "--", *patterns],
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    return [Path(line) for line in output.splitlines() if Path(line).is_file()]


def main() -> int:
    errors: list[str] = []
    files: list[Ftl] = []
    for path in tracked("crates/*/locales/*/*.ftl"):
        ftl, problems = parse(path, path.read_text(encoding="utf-8"))
        files.append(ftl)
        errors.extend(problems)
    sources = {path: path.read_text(encoding="utf-8") for path in tracked("crates/*.rs")}
    errors.extend(check(files, sources))
    for error in errors:
        print(f"FAIL: {error}")
    if errors:
        return 1
    count = sum(len(ftl.messages) for ftl in files)
    print(f"{count} messages in {len(files)} files are valid and used.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
