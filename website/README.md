# sheprd.org

The page Netlify publishes at <https://sheprd.org>. It is plain HTML, CSS and
JavaScript, with no install or build step. `site/` is separate and stays with
herdrui.dev.

```sh
python3 -m http.server -d website/site 8000
```

Python's server does not emulate Netlify headers or redirects. The Netlify
project uses base directory `website`, so it reads `website/netlify.toml`, which
publishes `website/site`. Only that folder is deployed, so this README and the
config stay private.

`herdrui/` is the separate Cloudflare Worker for herdrui.dev, and is not
published by Netlify; see [`herdrui/README.md`](herdrui/README.md).

`site/install.sh`, `site/install.ps1` and `site/llms.txt` here must match the
copies in the root `site/`,
and `site/tests/test_website.py` checks that. Keep `site/assets/features.js` in step
with `design/sheprd-website/FEATURE-INVENTORY.md`, and describe availability
honestly: status badges and platform caveats are part of the page.

Tests: `python3 -m unittest discover -s site/tests`.
