# Sheprd website

The current landing page for <https://sheprd.org>, published by Netlify from
`site/`. Root `netlify.toml` explicitly selects this directory and an empty
build command. It is plain HTML, CSS and JavaScript: no dependency install or
build tool is needed. Connect the repository `sheprd-labs/sheprd` to Netlify
and select its production branch separately; this configuration does not
deploy anything or change DNS.

```sh
python3 -m http.server -d site 8000
```

Open <http://localhost:8000/>. Python's server does not emulate Netlify headers
or redirects. `_headers` supplies security and cache headers, including
`text/plain` for both installer scripts. The only external page request is the
GitHub star count; failure leaves a plain GitHub link.

## Hosting and redirects

- Only `www.sheprd.org` redirects to `sheprd.org`; deploy-preview domains do
  not redirect to production.
- The retired Cloudflare Worker, Wrangler config, and `.assetsignore` are
  removed. Netlify does not interpret `.assetsignore`. Forced 404 rules block
  internal `README.md`, `tests/*`, and the retired Worker/config paths even
  if such files reappear. Keep these rules in sync when adding internal files.
- The former `.github/workflows/site.yml` Worker deploy workflow must be
  removed when switching hosting. Netlify handles static publishing.
- Redirecting `herdrui.dev` and `www.herdrui.dev` requires separate old-host
  configuration, domain verification, and DNS work. Preserve path and query
  string in a permanent redirect to `https://sheprd.org`; nothing here changes
  those hosts. Any old `pen.so/herdr-gpui/` proxy is also configured separately.

## Layout and copy

| File | Purpose |
| --- | --- |
| `index.html` | Product introduction, capabilities, optional media, installation |
| `names/index.html` | Selected-name and paw announcement, replacing the poll |
| `install.sh`, `install.ps1` | Checksummed latest-stable Linux and Windows installers |
| `llms.txt` | Plain-text capabilities, installation and compatibility notes |
| `assets/site.css`, `assets/site.js` | Page styling and behavior |
| `assets/kit.css`, `assets/kit.js` | Illustrative HTML app windows and media catalog |
| `assets/sheprd-avatar.svg`, `assets/sheprd-avatar.png` | Finalized paw avatar |
| `assets/sheprd-social-preview.png` | 1200×630 public social card |

This is a brand and accuracy pass on the current site, not a selection from
the separate website concepts. Describe Sheprd's own capabilities; do not
revive the old TUI-only comparison with Herdr Code, which has its own app.
Sheprd is independent of Herdr. Releases include upstream `herdr-core`;
users can also attach to their existing Herdr daemon. Existing sessions are
not automatically migrated, and closing the window leaves them running.

## Install compatibility

Release assets use `Sheprd-VERSION-TARGET` and the primary command is `sheprd`.
The Homebrew token remains `penso/tap/herdr-gpui`, with `Herdr.app` retained
for the update bridge (`open -a Herdr`). Linux's package name and legacy
`/usr/bin/herdr-gpui` entrypoint remain compatible. Windows keeps
`%LOCALAPPDATA%\Programs\Herdr UI`, `so.pen.herdr-gpui`, legacy executable
aliases, and both `sheprd://` and `herdr-gpui://` registrations. These strings
are compatibility identities, not current product branding. Keep installer
contracts aligned with `scripts/release/sheprd.iss` and its tests.

## Media and checks

App windows drawn by `kit.js` are labeled illustrations, not screenshots.
The film and gallery appear only when their files exist; `?draft` shows capture
placeholders. Capture new media from seeded demo sessions, never personal
terminal output. Screenshot WebPs use the IDs in `HM.SHOTS`; the film uses
`assets/film.mp4` and `assets/film-poster.webp`. Do not reuse old ram artwork
or present pre-rename captures as current branding.

```sh
node --check site/assets/site.js
node --check site/assets/kit.js
sh -n site/install.sh
python3 -m unittest discover -s site/tests
python3 -m unittest discover -s scripts/release/tests -p test_install_ps1.py
git diff --check -- site netlify.toml
```

Windows execution/registry checks need Windows PowerShell. Check desktop and
narrow layouts, theme switching, install tabs, copy buttons, links, and console
errors in a browser; Netlify response headers/redirects need a Netlify preview.
With Playwright available, `node site/tests/browser-check.cjs` checks the page
at desktop and phone widths using an isolated local server. `NODE_PATH` can
point to an existing Playwright installation; `SHEPRD_TEST_CHROMIUM` can select
an installed headless Chromium executable. Set `SHEPRD_SITE_SCREENSHOTS` to an
existing temporary directory to save light/dark captures for visual review.
