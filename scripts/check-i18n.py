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
