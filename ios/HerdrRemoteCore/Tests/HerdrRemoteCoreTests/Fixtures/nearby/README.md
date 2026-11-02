# Nearby pairing vectors

The code exchange of `docs/ios-app-plan.md`, "Nearby pairing, exactly", with
every key and nonce fixed. The Rust tests in
`crates/herdr-remote-control/src/auth/tests/nearby_vectors.rs` compute each
file from the daemon's own functions; the Swift tests must reach the same
values. Regenerate after a deliberate change with:

```sh
HERDR_UPDATE_FIXTURES=1 cargo test -p herdr-remote-control --locked nearby_vectors
```

## `session.json`

One exchange. Binary values are base64url without padding.

| Field | What |
| --- | --- |
| `phone_private_key`, `daemon_private_key` | P-256 private scalars (32 bytes) |
| `phone_public_key`, `daemon_public_key` | `PKa`, `PKb`: SEC1 uncompressed points (65 bytes) |
| `phone_nonce`, `daemon_nonce` | `Na`, `Nb` (32 bytes) |
| `fingerprint` | The CA fingerprint, 64 uppercase hex digits; its 32 bytes end `T` |
| `commit` | `SHA-256("herdr-sas-commit-v1" ‖ PKb ‖ PKa ‖ Nb)` |
| `code` | Six decimal digits from `SHA-256("herdr-sas-code-v1" ‖ T)` |
| `key` | `HKDF-SHA256(ikm: ECDH, salt: SHA-256(T), info: "herdr-sas-key-v1")` |
| `aes_nonce` | The AES-GCM nonce (12 bytes) |
| `plaintext` | The sealed JSON, exactly as sealed: `{"origins": [...], "token": ...}` |
| `sealed` | `aes_nonce ‖ AES-256-GCM(key, plaintext, aad: "herdr-sas-key-v1")` with the tag at the end |

`T = PKa ‖ PKb ‖ Na ‖ Nb ‖ fingerprint bytes`.

## `relay.json`

A man in the middle running one exchange with the phone (`phone_side`,
where it plays the daemon) and another with the daemon (`daemon_side`,
where it plays the phone), each with its own keys and nonces. Each side
has the fields of `session.json`, and the two `code`s differ.
