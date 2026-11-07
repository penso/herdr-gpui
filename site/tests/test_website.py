"""Publishing contracts for website/site, the page Netlify serves at sheprd.org."""
from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import urlsplit
import re
import struct
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
                    if target.is_dir():
                        target = target / "index.html"
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

    def test_page_has_no_media_label_pills(self):
        html = (WEBSITE / "index.html").read_text()
        css = (WEBSITE / "assets/base.css").read_text() + (WEBSITE / "assets/site.css").read_text()
        for leftover in ("tag-illustration", "tag-screenshot", "fig-tag", "lanes-tag"):
            with self.subTest(leftover=leftover):
                self.assertNotIn(leftover, html)
                self.assertNotIn(leftover, css)
        self.assertNotIn("Real screenshot", html)

    def test_masthead_links_no_page_sections(self):
        html = (WEBSITE / "index.html").read_text()
        nav = re.search(r"<nav .*?</nav>", html, re.S).group(0)
        self.assertEqual(re.findall(r'href="(#[^"]*)"', nav), ["#get"])

    def test_masthead_shows_the_repository_and_its_stars(self):
        html = (WEBSITE / "index.html").read_text()
        nav = re.search(r"<nav .*?</nav>", html, re.S).group(0)
        self.assertIn('<a class="repo" href="https://github.com/sheprd-labs/sheprd">', nav)
        # Hidden until the count arrives, so a failed request leaves a plain link.
        self.assertIn('id="stars" hidden', nav)
        self.assertIn('id="stars-n"', nav)
        self.assertIn("stargazers_count", (WEBSITE / "assets/site.js").read_text())

    def test_ios_app_links_to_the_public_testflight(self):
        html = (WEBSITE / "index.html").read_text()
        self.assertGreaterEqual(html.count('href="/testflight/"'), 3)
        self.assertNotIn("No public link", html)
        self.assertNotIn("no public link", html)
        page = (WEBSITE / "testflight/index.html").read_text()
        self.assertEqual(page.count('href="https://testflight.apple.com/join/6nZes3NY"'), 1)

    def test_the_testflight_invite_lives_only_on_its_page(self):
        # A new invite link is changed in one file; every other link goes there.
        invite = "testflight.apple.com/join/"
        for path in WEBSITE.rglob("*"):
            if path.is_file() and path.suffix in (".html", ".js", ".txt"):
                with self.subTest(file=str(path.relative_to(WEBSITE))):
                    count = path.read_text().count(invite)
                    self.assertEqual(count, 1 if path == WEBSITE / "testflight/index.html" else 0)
        app = (ROOT / "crates/sheprd-gpui/src/remote/dialog.rs").read_text()
        self.assertIn('"https://sheprd.org/testflight/"', app)
        self.assertIn("https://sheprd.org/testflight/", (ROOT / "README.md").read_text())

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

    def test_herdrui_worker_serves_current_installers_and_the_notice(self):
        worker = ROOT / "website/herdrui"
        self.assertEqual((worker / "public/index.html").read_bytes(),
                         (ROOT / "design/sheprd-website/herdrui-rename/index.html").read_bytes())
        source = (worker / "worker.js").read_text()
        for name in ("install.sh", "install.ps1", "llms.txt"):
            with self.subTest(file=name):
                self.assertIn(f"'/{name}'", source)
                self.assertEqual((worker / "public" / name).read_bytes(), (WEBSITE / name).read_bytes())

    def test_social_metadata_is_complete(self):
        html = (WEBSITE / "index.html").read_text()
        for tag in ('property="og:site_name"', 'property="og:image:width" content="1200"',
                    'property="og:image:height" content="630"', 'property="og:image:alt"',
                    'name="twitter:image"', 'name="twitter:title"', 'name="twitter:description"'):
            with self.subTest(tag=tag):
                self.assertIn(tag, html)
        description = re.search(r'<meta name="description" content="([^"]*)"', html).group(1)
        self.assertLessEqual(len(description), 160)

    def test_social_image_url_is_versioned_the_same_everywhere(self):
        html = (WEBSITE / "index.html").read_text()
        urls = re.findall(r'(?:property="og:image"|name="twitter:image") content="([^"]*)"', html)
        self.assertEqual(len(urls), 2)
        self.assertEqual(urls[0], urls[1], "bump ?v= on both tags")
        self.assertRegex(urls[0], r"\?v=\d+$")

    def test_herdrui_notice_announces_the_rename_when_shared(self):
        worker = ROOT / "website/herdrui"
        html = (worker / "public/index.html").read_text()
        urls = re.findall(r'(?:property="og:image"|name="twitter:image") content="([^"]*)"', html)
        self.assertEqual(len(urls), 2)
        self.assertEqual(urls[0], urls[1], "bump ?v= on both tags")
        parsed = urlsplit(urls[0])
        self.assertEqual(parsed.netloc, "herdrui.dev")
        self.assertRegex(parsed.query, r"^v=\d+$")
        self.assertIn(f"'{parsed.path}'", (worker / "worker.js").read_text())
        data = (worker / "public" / parsed.path.lstrip("/")).read_bytes()
        self.assertEqual(data[:8], b"\x89PNG\r\n\x1a\n")
        self.assertEqual(struct.unpack(">II", data[16:24]), (1200, 630))
        self.assertIn('property="og:url" content="https://herdrui.dev/"', html)

    def test_hero_commands_match_the_get_sheprd_section(self):
        html = (WEBSITE / "index.html").read_text()
        script = (WEBSITE / "assets/site.js").read_text()
        self.assertIn('id="hero-install"', html)
        self.assertIn('href="#get"', html)
        for command in ("brew install sheprd-labs/tap/sheprd",
                        "curl -fsSL https://sheprd.org/install.sh | sh",
                        "irm https://sheprd.org/install.ps1 | iex"):
            with self.subTest(command=command):
                self.assertIn(command, script)
                self.assertIn(command, html)


if __name__ == "__main__":
    unittest.main()
