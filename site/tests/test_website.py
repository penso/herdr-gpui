"""Publishing contracts for website/site, the page Netlify serves at sheprd.org."""
from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import urlsplit
import subprocess
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[2]
WEBSITE = ROOT / "website/site"


class Page(HTMLParser):
    def __init__(self, path):
        super().__init__()
        self.links = []
        self.ids = set()
        self.feed(path.read_text())

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if "id" in attrs:
            self.ids.add(attrs["id"])
        self.links += [attrs[key] for key in ("href", "src") if key in attrs]
        if attrs.get("property") == "og:image":
            self.links.append(attrs["content"])


class WebsiteTests(unittest.TestCase):
    def test_netlify_builds_website_site_from_its_own_config(self):
        config = tomllib.loads((ROOT / "website/netlify.toml").read_text())
        self.assertEqual(config["build"]["publish"], "site")
        self.assertNotIn("base", config["build"])
        result = subprocess.run(["bash", "-c", config["build"]["ignore"]], timeout=5)
        self.assertNotEqual(result.returncode, 0, "exit 0 would skip the build")
        canonical = [r for r in config["redirects"] if r["status"] == 301]
        self.assertEqual({r["from"] for r in canonical}, {
            "http://www.sheprd.org/*", "https://www.sheprd.org/*"})
        self.assertTrue(all(r["to"] == "https://sheprd.org/:splat" for r in canonical))

    def test_private_files_stay_out_of_the_published_folder(self):
        for name in ("README.md", "netlify.toml"):
            with self.subTest(file=name):
                self.assertFalse((WEBSITE / name).exists())

    def test_links_resolve_inside_the_published_folder(self):
        for path in WEBSITE.rglob("*.html"):
            page = Page(path)
            for link in page.links:
                parsed = urlsplit(link)
                if parsed.netloc and parsed.netloc != "sheprd.org":
                    continue
                with self.subTest(page=path.name, link=link):
                    if not parsed.path or parsed.path == "/":
                        target = path if not parsed.path else WEBSITE / "index.html"
                    elif parsed.path.startswith("/"):
                        target = WEBSITE / parsed.path.lstrip("/")
                    else:
                        target = path.parent / parsed.path
                    self.assertTrue(target.resolve().is_relative_to(WEBSITE), target)
                    self.assertTrue(target.is_file(), target)
                    if parsed.fragment:
                        self.assertIn(parsed.fragment, Page(target).ids)

    def test_installers_and_llms_match_site(self):
        for name in ("install.sh", "install.ps1", "llms.txt"):
            with self.subTest(file=name):
                self.assertEqual((WEBSITE / name).read_bytes(), (ROOT / "site" / name).read_bytes())

    def test_scripts_are_served_as_text(self):
        headers = (WEBSITE / "_headers").read_text()
        for name in ("install.sh", "install.ps1", "llms.txt"):
            with self.subTest(file=name):
                self.assertIn(f"/{name}\n  Content-Type: text/plain; charset=utf-8", headers)

    def test_page_carries_attribution_and_no_mockup_chrome(self):
        html = (WEBSITE / "index.html").read_text()
        self.assertIn("It is not made, endorsed or supported by the", html)
        for leftover in ("concept-ribbon", "Back to gallery", "../", "(future)"):
            with self.subTest(leftover=leftover):
                self.assertNotIn(leftover, html)

    def test_hidden_attribute_beats_author_display_rules(self):
        for base in (WEBSITE, ROOT / "design/sheprd-website"):
            with self.subTest(base=base.name):
                css = (base / "assets/base.css").read_text()
                self.assertIn("[hidden] { display: none !important; }", css)

    def test_credential_copy_keeps_the_teleport_exception(self):
        for base in (WEBSITE, ROOT / "design/sheprd-website"):
            with self.subTest(base=base.name):
                data = (base / "assets/features.js").read_text()
                self.assertNotIn("never leave", data)
                self.assertIn("Teleport can lend your GitHub CLI token", data)


if __name__ == "__main__":
    unittest.main()
