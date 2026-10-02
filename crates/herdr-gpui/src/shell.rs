//! POSIX shell quoting for commands this app types into a terminal or an SSH
//! session, where arguments must never become shell syntax.

/// Single-quote `value`: `'` inside becomes `'\''`.
pub(crate) fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
