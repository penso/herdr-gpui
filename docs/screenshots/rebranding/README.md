# Native Sheprd captures

These after captures were taken on macOS on 2026-11-03 from the isolated
native sidebar fixture, using its existing GPU capture facility with the
`integration-test,mockup` features. They contain fixture settings rather than
personal sessions. The capture run passed; the separate `just test-sidebar`
run passed all three tests.

- [Dark theme](settings-dark.png)
- [Light fonts page](settings-light.png)
- [Narrow sidebar settings](settings-narrow.png), including Sheprd labels

The [existing sidebar screenshot](../settings-sidebar.png) is historical
pre-rename context, not a newly captured before image at the same revision or
window size. No fresh before or native Dock/icon screenshot was captured.

An earlier run timed out while the open Settings window awaited its background
load. Later native runs passed, but the intermittent load delay's underlying
cause remains unresolved. Test-fixture fixes correct hit coordinates, menu
navigation and failure exit status; they do not claim to fix that delay.
