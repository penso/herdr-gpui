# Herdr Icon

The application icons combine Herdr's ram and terminal prompt with flat ivory
and graphite colors, without window controls, gradients, or shadows. The ram path is adapted from
[Herdr's logo](https://github.com/herdrdev/herdr/blob/HEAD/assets/logo.svg),
licensed under Apache-2.0. Changes include placement, scaling, flat colors,
and the surrounding tile or circle. See the root [NOTICE](../../NOTICE)
and [LICENSE](../../LICENSE) for distribution attribution and license terms.
Transparent margins keep the artwork aligned with other desktop icons.

| macOS / About box | Linux |
| --- | --- |
| <img src="herdr-ui-icon-clean.png" width="128" height="128" alt="Rounded Herdr icon"> | <img src="herdr-linux.svg" width="128" height="128" alt="Circular Herdr icon"> |

`herdr-ui-icon-clean.svg` is the rounded tile source artwork;
`herdr-icon-square-clean.svg` is the legacy full-square variant. It remains an
input to `scripts/generate-icons.swift`, which regenerates its normal and red
PNG exports; Linux packaging no longer installs those square assets.
The rounded 1024x1024 PNG export serves the About box; Linux packages install
`herdr-linux.svg`, a circular composition of the same ram, as the scalable icon.
On macOS, install the SVG renderer with
`brew install librsvg` and Xcode 26 or later, then run `just icons` after
changing the source SVG.
The generator uses `rsvg-convert` to rasterize the vector artwork directly at
each iconset resolution, rather than downsampling a PNG, and Apple's `iconutil`
to package `Herdr.icns`. It also compiles `Herdr.car` (see below) and
regenerates the PNG exports.
Assets are checked in, so ordinary builds do not require Swift, librsvg, or Xcode.

### Linux desktops

The tarball, Debian, RPM, Arch, and Nix packages install the circular SVG at
`share/icons/hicolor/scalable/apps/herdr-gpui.svg`. The circle and transparent
margins are part of the artwork: the desktop need not mask a square image.
The ram is smaller and centered, retaining the full silhouette inside the circle
rather than cropping the macOS composition. The circle is 896px across on a
1024px canvas, with 64px margins.
The desktop entry uses `Icon=herdr-gpui`; its filename and `StartupWMClass` match
the app's `so.pen.herdr-gpui` identity for Wayland and X11 window association.
Linked-worktree packages select the circular red `herdr-linux-worktree.svg`
instead, installed at the same scalable icon path.

Run `just icons-linux` on any platform to regenerate both Linux SVGs from the ram
path and attribution in `herdr-ui-icon-clean.svg`. It needs only Python's standard
library; the red palette matches the flat colors of the macOS worktree export.
`just icons` also runs this generator before generating the macOS assets.

This follows freedesktop icon lookup conventions used by GNOME, KDE Plasma, and
common launchers on Arch and Omarchy (Hyprland); no distribution-specific artwork
is needed. SVG support and icon-theme overrides belong to the desktop or launcher,
so identical rendering on every Linux setup is not guaranteed. A custom theme may
replace the icon. Tarball users must install the desktop entry and icon under an
XDG data prefix (such as `~/.local/share`); extracting the archive alone does not
register it. After replacing an installed icon, the launcher may need its icon
cache refreshed or a new desktop session before it displays the change.

To try the artwork locally without rebuilding or reinstalling the executable:

```sh
install -Dm644 assets/icons/herdr-linux.svg \
  ~/.local/share/icons/hicolor/scalable/apps/herdr-gpui.svg
gtk-update-icon-cache --force --ignore-theme-index ~/.local/share/icons/hicolor
```

This user-local icon overrides the packaged icon. Remove that SVG when you want
future artwork changes to come from package upgrades again, then refresh the cache.

### macOS sizing

The flattened `.icns` icons follow Apple's
[macOS Sequoia production template](https://devimages-cdn.apple.com/design/resources/download/macOS-Sequoia-Production-Templates-Sketch.dmg)
(`Template - Icon - App.sketch`). Its centered tile bounds are:

| Canvas (pixels) | Tile (pixels) | Margin on each side (pixels) |
| --- | --- | --- |
| 16 | 14 | 1 |
| 32 | 28 | 2 |
| 64 | 52 | 6 |
| 128 | 104 | 12 |
| 256 | 206 | 25 |
| 512 | 412 | 50 |
| 1024 | 824 | 100 |

The generator fits the source SVG's 896px rounded tile to these bounds before
rasterizing. Both the normal and red worktree icons use the same margins,
including the embedded 1024px PNG used by the About box. The circular Linux
artwork uses its own composition and margins instead of Apple's sizing.

All five logical sizes (16, 32, 128, 256, 512) include 1x and Retina 2x
representations, as described in Apple's
[high-resolution iconset guidance](https://developer.apple.com/library/archive/documentation/GraphicsAnimation/Conceptual/HighResolutionOSX/Optimizing/Optimizing.html).
Run `swift scripts/check-icons.swift` on macOS to verify the committed PNGs and
every representation extracted from both `.icns` files.

### macOS 26 and later

macOS 26 and later redraw a flattened `.icns` with Liquid Glass lighting, which
softens its edges and shades its flat colors in the Dock. Bundles therefore also
ship `Herdr.car` as `Contents/Resources/Assets.car`, selected by
`CFBundleIconName`. The generator builds it from an
[Icon Composer](https://developer.apple.com/design/human-interface-guidelines/app-icons)
document: the ivory tile is the document's solid fill, and the ram path from
`herdr-ui-icon-clean.svg` becomes a full-bleed vector layer. Glass, specular
highlights, translucency, and shadows are off, preserving the flat design.
`xcrun actool` compiles the document and its flattened fallbacks for older
systems, under the icon name `Herdr` for both variants. Test the rendering with
`NSWorkspace.icon(forFile:)` on a bundle, or in the Dock.

The same generator uses CoreImage to map each rendered image's luminance to a red
palette, retaining transparency, producing `herdr-worktree-1024.png`,
`herdr-square-worktree-1024.png`, `Herdr-worktree.icns`, and
`Herdr-worktree.car`; the catalog's two flat colors pass through the same mapping.
These derived assets identify linked-worktree builds;
the normal artwork remains unchanged. macOS development runs select the embedded
multi-resolution `.icns` at compile time for the Dock, while macOS/Linux packaging reads the executable's build
identity to select the matching icon, even when packaging in another checkout.

`plus.svg` and `close.svg` are original tab-control artwork, reused by the
workspace menu alongside the original `pencil.svg` (rename), `trash.svg`
(delete checkout) and `chevron-up.svg` / `chevron-down.svg` (fold and unfold a
worktree group); `git-branch.svg` is original artwork for the titlebar's Git
actions button; `sessions.svg` is original artwork for the sidebar footer's local
session list; `teleport.svg` is original artwork for moving a worktree to
another host, and `teleport-back.svg` its mirror for bringing it back; `zoom.svg` is original artwork marking a zoomed tab in the tab strip; `window-minimize.svg`, `window-maximize.svg`, and `window-restore.svg` are original artwork for the window buttons drawn on Linux when the compositor provides none, beside `close.svg`; `user.svg` is an
original generic silhouette for the future account placeholder, not a personal
identity or GitHub logo. All of them are embedded through a
minimal GPUI asset source. GPUI renders them as SVG masks tinted with the current
theme foreground, rather than fixed-color cached images.

`vscode.svg`, `error.svg`, and `pass.svg` are Codicons from Microsoft's
vscode-codicons, unchanged, under CC BY 4.0; `LICENSE-codicons` has the source
revision, the license, and the trademark notice. `vscode.svg` marks the VS Code
panel's title bar button and settings page; `error.svg` and `pass.svg` mark a
failed or working connection to its server.

## Sidebar agent marks

Each `agent-<label>.svg` is named after Herdr's canonical `agent_label`
identifier in `src/detect/mod.rs`, not an editable display label. The
`agent_icons!` table in `crates/herdr-gpui/src/icons.rs` is the only mapping;
its test checks that every label Herdr emits has its own mark. Unknown, empty,
and missing identities use `agent-generic.svg`.

| Label | Source |
| --- | --- |
| `pi` | Lobe Icons `pi.svg` (Pi Agent, pi.dev) |
| `claude` | Lobe Icons `claude.svg` |
| `codex` | Lobe Icons `openai.svg` |
| `gemini` | Lobe Icons `gemini.svg` |
| `cursor` | Lobe Icons `cursor.svg` |
| `devin` | Lobe Icons `devin.svg` |
| `agy` | Lobe Icons `antigravity.svg` |
| `cline` | Lobe Icons `cline.svg` |
| `mastracode` | Lobe Icons `mastra.svg` |
| `opencode` | Lobe Icons `opencode.svg` |
| `copilot` | Lobe Icons `githubcopilot.svg` |
| `kimi` | Lobe Icons `kimi.svg` |
| `kiro` | Lobe Icons `kiro.svg` |
| `amp` | Lobe Icons `amp.svg` |
| `grok` | Lobe Icons `grok.svg` |
| `hermes` | Lobe Icons `hermesagent.svg` |
| `kilo` | Lobe Icons `kilocode.svg` |
| `qodercli` | Lobe Icons `qoder.svg` |
| `qwen` | Lobe Icons `qwen.svg` |
| `omp`, `droid`, `letta`, `maki` | Original lettermark (O, D, L, M) in a rounded square |
| `muse` | Original lettermark (M) in a circle, to tell it apart from Maki |
| generic | Original terminal artwork |

Lobe Icons marks are MIT licensed; source attribution and the full license are
in the root [NOTICE](../../NOTICE), shipped with releases. Their SVG wrappers
were reduced to a 24px `currentColor` mask and titles removed; paths are
unchanged. No redistributable monochrome mark was found for oh-my-pi, Factory
Droid, Letta, Maki, or Muse, so their lettermarks and the generic mark are
original artwork under this project's Apache-2.0 license, drawn with the same
2px round stroke as the generic mark. All are embedded SVG masks tinted with
the adjacent name's theme color.
