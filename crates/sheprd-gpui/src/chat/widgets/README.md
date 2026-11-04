# Native chat cards

Put a JSON document like [example.json](example.json) inside a closed
`herdr-ui` Markdown fence in an agent reply. The preview renders a fixed native
GPUI catalog on Linux, macOS, and Windows. This is a small application-specific
format, not the A2UI protocol or an HTML/JavaScript execution environment.

```herdr-ui
{"title":"Choose a view","components":[{"type":"choice","id":"view","label":"View","options":["Summary","Details"]},{"type":"action","label":"Continue","prompt":"Explain the selected view","mode":"draft"}]}
```

| Component | Fields |
| --- | --- |
| `text` | `text` |
| `card` | `title`, `text` |
| `metric` | `label`, `value` (string), optional `detail` |
| `choice` | `id`, `label`, `options` (strings), optional `value` (defaults to first option) |
| `toggle` | `id`, `label`, optional boolean `value` |
| `slider` | `id`, `label`, `min`, `max`, `value`, optional `step` (default 1) |
| `input` | `id`, `label`, optional `placeholder` and initial `value` |
| `table` | `columns` (strings), `rows` (equally sized string arrays) |
| `chart` | `label`, `points` (objects with `label` and nonnegative numeric `value`); horizontal bars |
| `action` | `label`, `prompt`, optional `mode`: `draft` (default) or `send` |

The top level accepts `title` and `components`. Unknown keys, components and
invalid bounds reject the whole card, leaving its source readable. Unfinished
fences remain code. Editable ids must be unique ASCII letters/digits/underscores/
hyphens. Controls retain their values across repainting; replacing a card's
source or its session resets them.

Editing controls only changes local state. An action click appends the current
values as literal JSON to its prompt. **Add to message** preserves the existing
draft and lets the user review it. **Send to agent** uses the normal prompt flow;
it refuses a disconnected/read-only session or a nonempty existing draft, so it
cannot accidentally send another message or its attachments. Read-only transcript
mirrors still allow local exploration but cannot submit actions.

Limits: 32 KiB source, 32 components, 16 editable fields, 12 choices, 8 table
columns and 32 rows, 32 chart bars, 2 KiB input values, and a 16 KiB action prompt.
Up to 64 edited forms per pane are retained; at the cap, the oldest message's
form state is discarded. No control can name
an executable, network request, tool call, stylesheet, or event handler.
