# Windows local access investigation (#357)

Status: **unresolved on the reporting machine**. [Issue #357](https://github.com/penso/herdr-gpui/issues/357)
has no follow-up with the operation-specific errors requested by
[PR #361](https://github.com/penso/herdr-gpui/pull/361). That merged PR added
diagnostics and a default Windows log directory; it did not establish or fix
the access denial.

## What the current code requests

`crates/herdr-client/src/transport/windows.rs` connects through the locked
`interprocess` 2.4.6 byte-mode local stream. Its Windows implementation calls
`CreateFileW` with:

| Argument | Value |
| --- | --- |
| Name | `\\.\pipe\` followed by the discovered binary socket path |
| Desired access | `GENERIC_READ | GENERIC_WRITE` |
| Sharing | `FILE_SHARE_READ | FILE_SHARE_WRITE` |
| Creation | `OPEN_EXISTING` |
| Flags | `FILE_FLAG_OVERLAPPED` |
| Security attributes / template | null / null |

It waits with `WaitNamedPipeW` only after `ERROR_PIPE_BUSY`. The byte-mode,
blocking connector does **not** call `SetNamedPipeHandleState` to change the
read mode. Current code takes ownership of that handle, duplicates it with
the same access for the reader, and creates an event per I/O direction.
[PR #618](https://github.com/penso/herdr-gpui/pull/618) added overlapped I/O
deadlines; it retained the library's pipe-open path. The separate app-control
pipe introduced by [PR #620](https://github.com/penso/herdr-gpui/pull/620) is
not the daemon connector.

Herdr 0.9.3's own
[`ipc::connect_local_stream`](https://github.com/herdrdev/herdr/blob/v0.9.3/src/ipc.rs)
uses the same `GenericNamespaced` conversion and `LocalStream::connect` call.
Its private listener uses `D:P(A;;GA;;;SY)(A;;GA;;;OW)` (System and Owner Rights),
not a data-write-only grant. This is source evidence, not a measurement of the
pipe's effective security descriptor on the reporting machine.

The ordinary .NET `NamedPipeClientStream` with `PipeDirection.InOut` also
requests `GENERIC_READ | GENERIC_WRITE` and `OPEN_EXISTING`. Its default
synchronous open uses `FileShare.None` and no overlapped flag; selecting
`PipeOptions.Asynchronous` adds `FILE_FLAG_OVERLAPPED`. See
[.NET's Windows implementation](https://github.com/dotnet/runtime/blob/v9.0.0/src/libraries/System.IO.Pipes/src/System/IO/Pipes/NamedPipeClientStream.Windows.cs).
These differences warrant a same-machine comparison, not an assumption that
the GUI requests more access. `CanWrite` reports the stream's configured
direction, not a completed protocol exchange.

The config lock is a separate filesystem operation in
`crates/herdr-gpui/src/config/files.rs`. With the pinned Rust 1.96.1,
`read(true).write(true).create(true).truncate(false)` maps to
`GENERIC_READ | GENERIC_WRITE`, `OPEN_ALWAYS`, sharing read/write/delete, and
no custom flags. `File::lock()` then calls `LockFileEx` for an exclusive lock.
See [Rust's Windows filesystem implementation](https://github.com/rust-lang/rust/blob/1.96.1/library/std/src/sys/fs/windows.rs).
Creating a different file in the parent directory does not demonstrate access
to an existing lock file: that file has its own attributes, ACL, and open
handles. The two failures are not yet proven to have one cause.

[Windows' access-rights documentation](https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipe-security-and-access-rights)
explains that generic write includes several specific rights, including
`FILE_CREATE_PIPE_INSTANCE`. That applies equally to the ordinary .NET duplex
open. Reducing the requested mask without reproducing the failure, changing
the daemon's DACL, or bypassing the config lock is not a justified fix.

## Deterministic coverage

The tests use their own temporary paths and pipes, without a desktop or daemon:

```powershell
cargo test --locked -p herdr-client transport::tests::access
cargo test --locked -p herdr-gpui config::tests::lock_access
```

- A protected pipe DACL grants only the current user's generic read/write
  rights, rather than full control. The production connector must open it and
  exchange bytes both ways, using a daemon-style path with spaces and Unicode.
- The same exchange is exercised against Herdr 0.9.3's System/Owner Rights
  descriptor.
- A pipe explicitly denying write-data access must return native error 5,
  preserving `PermissionDenied`, even though a read-only open succeeds.
- An independently opened config-lock handle must fail a nonblocking lock
  attempt while maintenance owns the lock, then succeed after release.
- On Windows, a read-only existing lock must retain the lock path and native
  error 5, leaving the original config and migration destination untouched.

The pipe and read-only-attribute tests require **native Windows execution**.
A Linux test run excludes them; cross-target Clippy only checks compilation.
Even passing Windows fixtures would not reproduce the reporting installation's
token, daemon security descriptor, file attributes, or security software.

## Evidence still needed on the affected installation

1. Record the current GUI and daemon versions and reproduce once with normal
   discovery, then with the same explicit binary-client `--socket` path.
   Collect the nearby `operation`, `kind`, and `raw_os_error` log fields.
   The [GUI logging documentation](../crates/herdr-gpui/README.md#crash-reports)
   describes the records; no debug build or `RUST_LOG` is required.
2. For `pipe_connect`, capture the failing open/wait operation and the
   successful .NET open in Process Monitor, including desired access, sharing,
   creation disposition, options, result, and process identity/integrity level.
   Compare synchronous and asynchronous .NET opens against the **same pipe**
   from the same non-elevated shell. This distinguishes an open denial from a
   subsequent I/O failure; it does not require altering permissions.
3. For `config_lock_open`, inspect the existing `config-gpui.lock` file's
   attributes, ACL, and open handles as well as the parent directory. For
   `config_lock_acquire`, inspect the `LockFileEx` result instead. Preserve the
   failing file and its security state until the cause is understood.
4. Share only the relevant, redacted records. Verify a candidate fix on that
   installation and run the isolated tests above on Windows x64 and ARM64
   before calling this issue fixed.

The blocker is the missing native failure evidence, not an identified
upstream-only defect. The source comparison alone cannot establish which
Windows operation denied this installation's access.
