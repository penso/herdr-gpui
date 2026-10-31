# v2 wire fixtures

JSON the daemon's native API sends and reads (`docs/ios-app-plan.md`, "The
v2 wire, exactly"). The Rust tests in
`crates/herdr-remote-control/src/native/tests/` build each value from the
daemon's own types and compare it byte for byte with these files; the
Swift tests decode them. Regenerate after a deliberate change with:

```sh
HERDR_UPDATE_FIXTURES=1 cargo test -p herdr-remote-control --locked native::tests::
```

| File | What |
| --- | --- |
| `home.json`, `chat_snapshot.json`, `chat_resumed.json`, `chat_changes.json`, `chat_connection.json`, `term_screen.json`, `reload.json`, `error.json`, `bye.json` | Live socket messages from the daemon |
| `subscribe_home.json`, `subscribe_chat.json`, `subscribe_term.json`, `unsubscribe_chat.json` | Live socket messages from the phone |
| `chat.json` | `GET /api/v2/chat/{pane}`: a `chat_snapshot` without its `type` |
| `hosts.json`, `host_state.json` | `GET /api/v2/hosts` with one host in each state, and the bare state `POST /api/v2/hosts/{host}/pair` answers |
| `reducer/*.json` | Reducer vectors |

## Reducer vectors

Each is `{"start": Transcript, "changes": [Change], "end": Transcript}`:
applying every change to `start`, in order, with the protocol's reducer
gives `end`. A `rejected_*` vector also has `"error"`: every change but
the last applies, the last fails with that error, and the transcript is
left as it was, so `end` is the state after the changes that applied.

Error names: `unknown_entry`, `entry_order`, `not_text`,
`unknown_permission`, `duplicate_permission`, `no_modes`,
`question_answers_limit`.
