# herdrui.dev

The Cloudflare Worker `herdrui-site`, which answers herdrui.dev and
www.herdrui.dev. It is not part of the sheprd.org site: Netlify publishes only
`website/site`.

- `/install.sh`, `/install.ps1` and `/llms.txt` serve the same files as
  sheprd.org, so install commands from old docs get the Sheprd release.
- `www.herdrui.dev` redirects to herdrui.dev, keeping the path.
- Every other path shows the rename notice, `public/index.html`, which opens
  the same path on sheprd.org after five seconds.

- `/assets/rename-social.png` is the card shown when herdrui.dev is shared. Its
  source is `rename-social.svg`: render it with
  `rsvg-convert rename-social.svg -o public/assets/rename-social.png`, then
  raise `?v=` on `og:image` and `twitter:image` so crawlers fetch it again.

The notice must stay identical to
`design/sheprd-website/herdrui-rename/index.html`; `site/tests/test_website.py`
checks that.

The scripts must also match `website/site`, which the same test checks. After
changing them, deploy this Worker again.

## Deploy

Nothing deploys this automatically. Use a Cloudflare API token from the **Edit
Cloudflare Workers** template, limited to the herdrui.dev zone:

```sh
cd website/herdrui
export CLOUDFLARE_API_TOKEN=... CLOUDFLARE_ACCOUNT_ID=...
npx wrangler@4.144.0 deploy --dry-run
npx wrangler@4.144.0 deploy
```

`npx wrangler@4.144.0 dev` serves it locally. On localhost the notice stops at
zero rather than leaving the page.

To undo a deploy, run `npx wrangler@4.144.0 rollback` and pick a version from
`npx wrangler@4.144.0 deployments list --name herdrui-site`. The last version
before the rename notice is `8e42d422-f023-4746-b2bb-3a2131aa5283`.
