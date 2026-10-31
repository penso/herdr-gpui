//! The two scripts machine sync runs on each machine over SSH, and reading
//! their replies. The first reads (creating the herdr key if needed); the
//! second writes, and only if the machine's catalog is still the one read.
use super::{Error, SyncState, keys::PublicKey, parse_state};
use crate::{
    SshLane,
    catalog::{Catalog, parse_catalog},
    script::{ScriptHost, ScriptLimits, run_script, shell_quote},
};
use std::{sync::atomic::AtomicBool, time::Duration};

/// A catalog and a state file of their largest sizes, and the lines around them.
const OUTPUT_LIMIT: u64 = 160 * 1024;
const LIMITS: ScriptLimits = ScriptLimits {
    output: OUTPUT_LIMIT,
    idle: Duration::from_secs(30),
};

/// Where Herdr keeps the catalog on a POSIX machine, as `catalog_path` says.
pub(crate) const PATHS: &str = r#"set -eu
umask 077
ssh_dir="$HOME/.ssh"
key="$ssh_dir/herdr_ed25519"
dir="${XDG_STATE_HOME:-$HOME/.local/state}/herdr/client"
catalog="$dir/endpoints.json"
state="$dir/machine-sync.json"
"#;

/// Creates the herdr key when missing, then prints the public key, the
/// catalog's checksum, the catalog, and the machine's own sync state.
pub(crate) const COLLECT: &str = r#"[ -d "$ssh_dir" ] || mkdir -m 700 "$ssh_dir"
if [ ! -f "$key" ]; then
    ssh-keygen -q -t ed25519 -N '' -C herdr-sync -f "$key" </dev/null >/dev/null
fi
if [ ! -f "$key.pub" ]; then
    ssh-keygen -y -f "$key" </dev/null >"$key.pub"
fi
section() {
    if [ -f "$2" ]; then
        content=$(cat "$2")
        printf '%s %s\n%s\n' "$1" "$(printf '%s' "$content" | wc -c | tr -d ' ')" "$content"
    else
        printf '%s missing\n' "$1"
    fi
}
printf 'herdr-sync:1\n'
printf 'key %s\n' "$(head -n 1 "$key.pub")"
if [ -f "$catalog" ]; then
    printf 'sum %s\n' "$(cksum <"$catalog")"
else
    printf 'sum missing\n'
fi
section catalog "$catalog"
section state "$state"
"#;

/// What a machine reported about itself.
pub(crate) struct Collected {
    pub(crate) key: PublicKey,
    /// The catalog's `cksum`, or `missing`, which the write must still match.
    pub(crate) sum: String,
    pub(crate) catalog: Catalog,
    /// The machine's own sync state, when it has a valid one.
    pub(crate) state: Option<SyncState>,
}

/// Reads `target`. Blocks on SSH: call it from a background worker.
pub(crate) fn collect(target: &str, cancelled: &AtomicBool) -> Result<Collected, Error> {
    let script = format!("{PATHS}{COLLECT}");
    let output = run(target, &script, &[], cancelled)?;
    parse_collected(&output)
}

/// What the write changes on one machine.
pub(crate) struct Write<'a> {
    /// The `cksum` the catalog must still have.
    pub(crate) sum: &'a str,
    /// `authorized_keys` lines to add when their key is missing.
    pub(crate) keys: &'a [String],
    /// The new catalog, or `None` to leave it.
    pub(crate) catalog: Option<&'a [u8]>,
    /// The state to save when the machine has none.
    pub(crate) state: Option<&'a str>,
}

/// Writes `write` on `target` and returns how many keys were added; refused
/// with [`Error::Changed`] when the catalog changed since it was read.
pub(crate) fn apply(target: &str, write: &Write, cancelled: &AtomicBool) -> Result<usize, Error> {
    let script = format!("{PATHS}{}", apply_script(write));
    let output = run(
        target,
        &script,
        write.catalog.unwrap_or_default(),
        cancelled,
    )?;
    let text = std::str::from_utf8(&output).map_err(|_| Error::Reply)?;
    match text.trim_end().strip_prefix("herdr-sync:") {
        Some("changed") => Err(Error::Changed),
        Some(done) => done
            .strip_prefix("done ")
            .and_then(|count| count.parse().ok())
            .ok_or(Error::Reply),
        None => Err(Error::Reply),
    }
}

pub(crate) fn apply_script(write: &Write) -> String {
    let keys: Vec<String> = write.keys.iter().map(|line| shell_quote(line)).collect();
    let catalog = if write.catalog.is_some() {
        r#"tmp="$dir/.endpoints-sync.$$"
cat >"$tmp"
mv -f "$tmp" "$catalog"
"#
    } else {
        ""
    };
    let state = match write.state {
        Some(json) => format!(
            r#"if [ ! -e "$state" ]; then
    tmp="$dir/.machine-sync.$$"
    printf '%s' {} >"$tmp"
    mv -f "$tmp" "$state"
fi
"#,
            shell_quote(json)
        ),
        None => String::new(),
    };
    format!(
        r#"current=missing
if [ -f "$catalog" ]; then current=$(cksum <"$catalog"); fi
if [ "$current" != {sum} ]; then
    printf 'herdr-sync:changed\n'
    exit 0
fi
mkdir -p "$dir"
[ -d "$ssh_dir" ] || mkdir -m 700 "$ssh_dir"
auth="$ssh_dir/authorized_keys"
if [ -s "$auth" ] && [ -n "$(tail -c 1 "$auth")" ]; then printf '\n' >>"$auth"; fi
added=0
set -- {keys}
for line do
    if ! grep -qF -- "${{line% *}}" "$auth" 2>/dev/null; then
        printf '%s\n' "$line" >>"$auth"
        added=$((added + 1))
    fi
done
{catalog}{state}printf 'herdr-sync:done %s\n' "$added"
"#,
        sum = shell_quote(write.sum),
        keys = keys.join(" "),
    )
}

fn run(target: &str, script: &str, input: &[u8], cancelled: &AtomicBool) -> Result<Vec<u8>, Error> {
    let mut output = Vec::new();
    run_script(
        ScriptHost::Ssh(target, SshLane::Quick),
        script,
        input,
        &mut output,
        LIMITS,
        cancelled,
    )
    .map_err(|error| Error::Script(Box::new(error)))?;
    Ok(output)
}

/// Reads the collect script's reply.
pub(crate) fn parse_collected(output: &[u8]) -> Result<Collected, Error> {
    let mut reply = Reply(output);
    if reply.line()? != b"herdr-sync:1" {
        return Err(Error::Reply);
    }
    let key = reply.field("key")?;
    let key = PublicKey::parse(std::str::from_utf8(key).map_err(|_| Error::Reply)?)?;
    let sum = std::str::from_utf8(reply.field("sum")?).map_err(|_| Error::Reply)?;
    let valid_sum = sum == "missing"
        || (sum.len() <= 64 && sum.bytes().all(|b| b.is_ascii_digit() || b == b' '));
    if !valid_sum {
        return Err(Error::Reply);
    }
    let catalog = match reply.section("catalog")? {
        Some(bytes) => parse_catalog(bytes).map_err(|error| Error::Catalog(Box::new(error)))?,
        None => Catalog {
            version: 1,
            selected_profile: None,
            ssh: Vec::new(),
        },
    };
    // A machine whose own state cannot be read is synced as one without it;
    // the write never replaces a state file that exists.
    let state = reply
        .section("state")?
        .and_then(|bytes| parse_state(bytes).ok());
    Ok(Collected {
        key,
        sum: sum.to_owned(),
        catalog,
        state,
    })
}

struct Reply<'a>(&'a [u8]);

impl<'a> Reply<'a> {
    fn line(&mut self) -> Result<&'a [u8], Error> {
        let end = self
            .0
            .iter()
            .position(|&b| b == b'\n')
            .ok_or(Error::Reply)?;
        let (line, rest) = self.0.split_at(end);
        self.0 = &rest[1..];
        Ok(line)
    }

    /// The rest of a `name value` line.
    fn field(&mut self, name: &str) -> Result<&'a [u8], Error> {
        let line = self.line()?;
        line.strip_prefix(name.as_bytes())
            .and_then(|rest| rest.strip_prefix(b" "))
            .ok_or(Error::Reply)
    }

    /// A `name LENGTH` line and that many bytes and a newline after it, or
    /// `None` for `name missing`.
    fn section(&mut self, name: &str) -> Result<Option<&'a [u8]>, Error> {
        let value = self.field(name)?;
        if value == b"missing" {
            return Ok(None);
        }
        let length: usize = std::str::from_utf8(value)
            .ok()
            .and_then(|text| text.parse().ok())
            .ok_or(Error::Reply)?;
        if self.0.len() <= length || self.0[length] != b'\n' {
            return Err(Error::Reply);
        }
        let (content, rest) = self.0.split_at(length);
        self.0 = &rest[1..];
        Ok(Some(content))
    }
}
