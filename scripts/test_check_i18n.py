"""Tests for scripts/check-i18n.py."""

import importlib.util
import sys
import unittest
from pathlib import Path

# Loading the hyphenated script as a module would leave a __pycache__ beside it.
sys.dont_write_bytecode = True
SPEC = importlib.util.spec_from_file_location(
    "check_i18n", Path(__file__).with_name("check-i18n.py")
)
assert SPEC and SPEC.loader
check_i18n = importlib.util.module_from_spec(SPEC)
# Dataclasses look their module up by name.
sys.modules["check_i18n"] = check_i18n
SPEC.loader.exec_module(check_i18n)

EN = Path("crates/app/locales/en-US/menu.ftl")


def parse(text: str, path: Path = EN):
    return check_i18n.parse(path, text)


class ParseTests(unittest.TestCase):
    def test_messages_comments_and_multiline_values_parse(self) -> None:
        ftl, errors = parse(
            "# Menu text.\n"
            "\n"
            "menu-quit = Quit\n"
            "menu-tabs = { $count ->\n"
            "    [one] One tab\n"
            "   *[other] { $count } tabs\n"
            "}\n"
            "menu-long =\n"
            "    First line\n"
            "\n"
            "    second line\n"
            "    .title = { menu-quit }\n"
            "-brand = Herdr\n"
        )
        self.assertEqual(errors, [])
        self.assertEqual(list(ftl.messages), ["menu-quit", "menu-tabs", "menu-long"])
        self.assertEqual(ftl.references, {"menu-quit"})

    def test_ids_take_the_feature_prefix_once(self) -> None:
        _, errors = parse("menu-quit = Quit\nquit = Quit\nmenu-quit = Again\n")
        self.assertEqual(len(errors), 2)
        self.assertIn(":2: quit must start with menu-", errors[0])
        self.assertIn(":3: menu-quit is defined twice", errors[1])

    def test_braces_in_string_literals_are_text(self) -> None:
        ftl, errors = parse(
            'menu-brace = { "{" }\n'
            'menu-close = { "}" } and { "\\"}" }\n'
            'menu-quoted = Say "{ $name }"\n'
        )
        self.assertEqual(errors, [])
        self.assertEqual(list(ftl.messages), ["menu-brace", "menu-close", "menu-quoted"])
        _, errors = parse("menu-quit = Quit }\n")
        self.assertEqual(errors, [f"{EN}:1: unbalanced }}"])

    def test_quotes_in_select_branches_are_text(self) -> None:
        ftl, errors = parse(
            "menu-count = { $count ->\n"
            "    [one] One item\n"
            '   *[other] Items measured in "inches }\n'
            "menu-size = { NUMBER($size, style: \"unit\") ->\n"
            '    [one] One "{ $size }" unit\n'
            '   *[other] { $size } units, { "}" } and "more\n'
            "}\n"
            "menu-term = { -brand } \"quoted\" { -brand-name }\n"
        )
        self.assertEqual(errors, [])
        self.assertEqual(list(ftl.messages), ["menu-count", "menu-size", "menu-term"])
        _, errors = parse('menu-open = { $count ->\n   *[other] "Open\n')
        self.assertEqual(errors, [f"{EN}: a placeable is not closed"])

    def test_stray_lines_and_open_placeables_are_reported(self) -> None:
        _, errors = parse("menu-quit = Quit\nnot a message\n")
        self.assertEqual(errors, [f"{EN}:2: not a message, comment, or continuation"])
        _, errors = parse("menu-quit = { $name\n")
        self.assertEqual(errors, [f"{EN}: a placeable is not closed"])


class CheckTests(unittest.TestCase):
    def files(self, *texts: tuple[Path, str]):
        return [parse(text, path)[0] for path, text in texts]

    def test_examples_in_comments_are_not_calls(self) -> None:
        files = self.files((EN, "menu-quit = Quit\n"))
        sources = {Path("a.rs"): '/// `fl!("id")`\nfl!("menu-quit")'}
        self.assertEqual(check_i18n.check(files, sources), [])

    def test_used_messages_pass(self) -> None:
        files = self.files((EN, "menu-quit = Quit\nmenu-all = All { menu-quit }\n"))
        sources = {Path("a.rs"): 'fl!("menu-all")'}
        self.assertEqual(check_i18n.check(files, sources), [])

    def test_a_table_literal_counts_as_a_use(self) -> None:
        files = self.files((EN, "menu-quit = Quit\n"))
        sources = {Path("a.rs"): 'message: "menu-quit",'}
        self.assertEqual(check_i18n.check(files, sources), [])

    def test_a_message_only_tests_name_is_unused(self) -> None:
        files = self.files((EN, "menu-quit = Quit\nmenu-open = Open\n"))
        sources = {
            Path("crates/app/src/menu.rs"): 'fl!("menu-quit")',
            Path("crates/app/src/menu/tests.rs"): 'fl!("menu-open")',
            Path("crates/app/src/menu/tests/keys.rs"): '"menu-open"',
            Path("crates/app/src/layout_tests/rows.rs"): '"menu-open"',
            Path("crates/app/src/interaction_tests.rs"): '"menu-open"',
            Path("crates/app/tests/cli.rs"): '"menu-open"',
        }
        self.assertEqual(
            check_i18n.check(files, sources), [f"{EN}:2: menu-open is not used"]
        )

    def test_a_test_still_cannot_name_a_missing_message(self) -> None:
        files = self.files((EN, "menu-quit = Quit\n"))
        sources = {
            Path("crates/app/src/menu.rs"): 'fl!("menu-quit")',
            Path("crates/app/src/menu/tests.rs"): 'fl!("menu-gone")',
        }
        self.assertEqual(
            check_i18n.check(files, sources),
            ['crates/app/src/menu/tests.rs: fl!("menu-gone") names no en-US message'],
        )

    def test_unused_and_unknown_messages_are_reported(self) -> None:
        files = self.files((EN, "menu-quit = Quit\n"))
        sources = {Path("a.rs"): 'fl!("menu-open")'}
        self.assertEqual(
            check_i18n.check(files, sources),
            [
                'a.rs: fl!("menu-open") names no en-US message',
                f"{EN}:1: menu-quit is not used",
            ],
        )

    def test_an_id_repeated_across_files_and_a_stale_translation_fail(self) -> None:
        other = Path("crates/app/locales/en-US/menu2.ftl")
        french = Path("crates/app/locales/fr/menu.ftl")
        files = self.files((EN, "menu-quit = Quit\n"), (other, "menu-quit = Quit\n"))
        files[1].feature = "menu"
        files += self.files((french, "menu-gone = Parti\n"))
        errors = check_i18n.check(files, {Path("a.rs"): '"menu-quit"'})
        self.assertIn(f"{other}:1: menu-quit is also defined in {EN}", errors)
        self.assertIn(f"{french}: menu-gone has no en-US message", errors)


class MigratedTests(unittest.TestCase):
    ROOT = Path("crates/app/src/menu.rs")

    def errors(self, sources: dict[Path, str]) -> list[str]:
        errors = []
        for path in sorted(check_i18n.migrated(sources)):
            errors.extend(check_i18n.shown_english(path, sources[path]))
        return errors

    def test_unmarked_modules_are_not_checked(self) -> None:
        self.assertEqual(self.errors({self.ROOT: 'label("Quit Herdr")'}), [])

    def test_a_marked_module_and_its_directory_reject_english(self) -> None:
        sources = {
            self.ROOT: '// i18n: migrated\nlabel("Quit Herdr");\nfl!("menu-quit")',
            Path("crates/app/src/menu/row.rs"): 'let a = "open the file";\nlet b = "x-y";',
            Path("crates/app/src/menu/tests/row.rs"): 'label("Quit")',
            Path("crates/app/src/other.rs"): 'label("Quit")',
        }
        self.assertEqual(
            self.errors(sources),
            [
                'crates/app/src/menu/row.rs:1: "open the file" is not a message',
                'crates/app/src/menu.rs:2: "Quit Herdr" is not a message',
            ],
        )

    def test_logs_attributes_and_data_are_allowed(self) -> None:
        source = (
            "// i18n: migrated\n"
            '#[error("Could not read {path}")]\n'
            'tracing::warn!(\n    "Unable to open {}",\n    path,\n);\n'
            'write!(f, "Could not save")?;\n'
            'value.expect("Present in tests");\n'
            '.key_context("SettingsWindow")\n'
            'let font = "SF Mono"; // i18n: data\n'
            "// i18n: data\n"
            'let theme = "Catppuccin Mocha";\n'
            "// Comments say \"Quit Herdr\".\n"
            "let c = '\"'; let lifetime: &'static str = \"ok\";\n"
            'let shown = format!("Open {name}");\n'
        )
        self.assertEqual(
            self.errors({self.ROOT: source}),
            ['crates/app/src/menu.rs:15: "Open {name}" is not a message'],
        )

    def test_placeholders_and_raw_strings(self) -> None:
        source = '// i18n: migrated\nformat!("{Name} {count}");\nlet r = r#"Raw text"#;\n'
        self.assertEqual(
            self.errors({self.ROOT: source}),
            ['crates/app/src/menu.rs:3: "Raw text" is not a message'],
        )

    def test_unicode_names(self) -> None:
        source = '// i18n: migrated\nlet café = 1;\nnaïve::größe!("Open file");\n'
        self.assertEqual(
            self.errors({self.ROOT: source}),
            ['crates/app/src/menu.rs:3: "Open file" is not a message'],
        )


if __name__ == "__main__":
    unittest.main()
