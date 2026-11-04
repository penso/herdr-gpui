# Sheprd naming and compatibility

The product is **Sheprd**, the main command is **`sheprd`**, the primary
repository is [`sheprd-labs/sheprd`](https://github.com/sheprd-labs/sheprd), and
the site is [sheprd.org](https://sheprd.org). Owned crates use `sheprd-*`;
`sheprd-gpui` builds `sheprd`, and `sheprd-launcher` is the Windows launcher.
The macOS payload is `Sheprd.app` with `Contents/MacOS/sheprd`. Swift modules
and directories use `SheprdRemote*`; the visible phone/watch app is simply Sheprd.

This source rename follows the deployed reader bridge described in
[App Updates](updating.md#sheprd-compatibility-rollout): release
[`v20261103.1`](https://github.com/penso/herdr-gpui/releases/tag/v20261103.1),
published on 2026-11-03 from `063e7e1533d0a8e52d1df0f6e2774ba5c1cc8c7a`.
It does not itself
transfer a GitHub repository, deploy a domain, publish a release, update a tap,
or submit an App Store build. The existing website receives basic rebranding;
the separate website design exploration has no chosen design documented here.
Existing screenshots remain historical captures, not evidence of refreshed UI.

## Intentional legacy inventory

| Surface | Retained contract |
| --- | --- |
| Update trust | Same Ed25519 key; manifest/signature filenames, schemas 1–4, target/count/size bounds and platform verification remain unchanged. Both `penso/herdr-gpui` and `sheprd-labs/sheprd` are accepted repository identities. |
| macOS | Bundle ID `so.pen.herdr-gpui`, installed signing team/Developer ID requirement and in-place update path. A new `Sheprd.app` payload can replace `/Applications/Herdr.app` without moving that directory. |
| Homebrew | New installs use `sheprd` in `sheprd-labs/tap`, installing `Sheprd.app`. Existing installs keep token `herdr-gpui` in `penso/tap`, which Homebrew will not move automatically; visible name Sheprd; `app "Sheprd.app", target: "Herdr.app"`. No cask-token or destination migration. |
| Linux packages | Internal owner/name `herdr-gpui`, version `VERSION-1`, and a real root-owned `/usr/bin/herdr-gpui` for the existing privileged verifier/restart contract, alongside canonical `sheprd`. |
| Linux standalone | The manual archive links `bin/herdr-gpui` to `sheprd`, so replacing the resolved executable updates both command names. Distro packages materialize the regular legacy executable required above. |
| Nix | Existing `herdr-gpui` package/overlay attributes and legacy main/helper commands remain aliases. The main command alias targets the final wrapped `sheprd` executable. |
| Windows | `%LOCALAPPDATA%\Programs\Herdr UI`, Inno AppId `{7FA70894-4FC0-4923-BDCB-B05F7475610A}`, AppUserModelID `so.pen.herdr-gpui`, winget `Penso.HerdrUI`, and the same Authenticode publisher. Keep `herdr-gpui.exe` / `herdr-gpui-launcher.exe` alongside `sheprd.exe` / `sheprd-launcher.exe`; visible installer and shortcuts say Sheprd. |
| Saved data | Existing config, state, cache, credential-store and service identities remain. Examples include `herdr/`, `config-gpui.toml`, `.herdr/`, `herdr-automate` and `herdr-remote-control` storage roots, OS keyring service names, launchd/systemd labels and Windows service/task identities. Do not move or delete these as branding cleanup. |
| Existing integrations | Preserve control socket/pipe identities, `herdr-gpui://` and `herdr-remote://` links, browser `HerdrKit` / `herdr.send` / `/.herdr/` APIs, Luau `herdr.*`, `@herdr/ui`, `herdr-ui` chat fences and existing environment/config keys. Managed skill YAML names/folders remain `herdr-gpui-browser`, `herdr-automations` and `herdr-mods`; their marker remains `herdr-gpui-managed-skill v1`. Descriptions and CLI examples use Sheprd without inventing renamed extension APIs. |
| iOS/watchOS | Production `so.pen.herdr.remote` app/extension/watch IDs, companion identifier, signing team, `group.so.pen.herdr.remote` App Group, Keychain services/access groups, APNs topics/entitlements, Bonjour `_herdr-remote._tcp`, Bluetooth and pairing/crypto identities remain. Module renames do not authorize changing these. |
| Upstream | Herdr, herdr-core, `hc`, daemon CLI commands, sockets, wire contracts and third-party licensing attribution keep their actual names. Sheprd remains unaffiliated with Herdr. |

## Release and installation contracts

New publication emits `sheprd-VERSION-...` updater archives,
`Sheprd-VERSION-...` manual DMGs/archives/packages/installers, and
`sheprd-remote-control-VERSION-TARGET` host binaries. Preserve the precise
[accepted asset and installation contracts](updating.md#accepted-names-and-installed-layouts):
case matters, aliases must not duplicate a target/format slot, and accepted asset
names do not relax signature, extraction, ownership or installed-path checks.

Linux packages additionally retain byte-identical `Herdr-VERSION-TARGET`
download aliases. The signed package manifest lists the legacy name only, once
per format and target: the deployed `v20261103.1` bridge accepts Sheprd names
while selecting an offer but still requires the legacy filename when preparing
a package download. Current clients accept either spelling at both steps.
Keep these publication aliases until bridge installations have a tested upgrade
path; updating this source cannot repair that check in an already installed binary.

Use `sheprd-icon.png`, `sheprd-icon-badge.png`, the `sheprd-icon-square*` family
and `Sheprd.icns` / `.ico` / `.car` for canonical paw artwork references. Retain
third-party code and service-mark attribution in [NOTICE](../NOTICE).

The deployed bridge is legacy-first: a successful legacy response is authoritative;
only a 404 retries the same resource once at the successor. Other failures never
choose another feed. Renamed clients use the new primary repository. Keep immutable
old releases and version-pinned host builds available, and preserve GitHub's transfer
redirects. Recreating the old repository can remove those redirects and strand
bridge clients on an authoritative stale feed.

**Clients that missed the bridge may reject successor URLs and renamed assets.**
They may need a manually verified installation or package-manager upgrade; the
rename cannot patch an offline old binary. Publish explicit recovery instructions
before changing external release routing. Historical Sigstore certificates retain
their original workflow identity; the updater's Ed25519 trust anchor does not change.

## Manual rollout and QA checklist

These are release-owner tasks, not claims of completed validation or deployment.

- [ ] Record the deployed bridge version and a recovery path for pre-bridge clients;
  retain old release assets and test their version-pinned helper downloads.
- [ ] Review repository transfer/permissions, protected environments, secrets,
  OIDC/provenance policy, webhooks and redirects. Reuse the existing signing keys.
- [ ] Deploy `sheprd.org` install scripts/download links and preserve or redirect
  legacy domain URLs. Verify external repository/domain changes separately.
- [ ] Update the stable cask in both taps with its legacy token and install target;
  retain winget `Penso.HerdrUI`. Do not rename package-manager identities implicitly.
- [ ] Exercise signed two-version upgrades from the bridge on both macOS
  architectures: standalone, Homebrew stable and beta; confirm preserved path,
  Dock identity, signature validation, relaunch, cancellation and rollback.
- [ ] On Debian/Ubuntu, Fedora and Arch, upgrade through the package manager;
  verify owner `herdr-gpui`, both executable names, PolicyKit, restart and removal.
  Test Linux standalone replacement at both old and canonical executable paths.
- [ ] On x86_64 and ARM64 Windows, test upgrade/relaunch/uninstall, both executable
  pairs, existing pins, notifications, folder shortcuts, URL handling and winget
  version detection at the preserved installation directory.
- [ ] Confirm settings, saved sign-ins, cloud devices, automations, mods and remote
  pairings survive; test existing scripts/managed skills and both CLI entry points
  where compatibility aliases are shipped. Daemon sessions must keep running.
- [ ] Test existing iOS/watchOS pairings, shared data, Keychain, push, Bonjour,
  nearby presence and companion upgrades with production identities intact.
  App Store/TestFlight names, metadata and submissions require a separate rollout.
- [ ] Capture the real renamed desktop and phone UI, including light/dark and
  narrow layouts, before presenting screenshots as current. Review icon/license
  notices and links against the final moved files.
- [ ] Run the integrated code/package gates and record exact results; documentation
  review alone does not prove native upgrades or external deployment.
