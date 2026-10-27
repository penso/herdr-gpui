//! Bounded Git work on the saved SSH host that owns a picker target.

use super::Item;
use herdr_client::shell_quote;
use std::time::Instant;

pub(super) fn directory(path: &str) -> crate::Result<&str> {
    if !path.starts_with('/') || path.chars().any(char::is_control) {
        return Err(herdr_client::Error::InvalidGitDir.into());
    }
    Ok(path)
}

pub(super) fn fetch_script(path: &str, item: &Item) -> crate::Result<String> {
    // Let Git choose the host's configured key or SSH wrapper. Only supply
    // OpenSSH defaults when neither the environment nor Git config names one.
    // The remote command has no TTY; disable graphical SSH prompts as well.
    Ok(format!(
        r#"set -eu
export GIT_TERMINAL_PROMPT=0 GCM_INTERACTIVE=never SSH_ASKPASS_REQUIRE=never
directory={}
if [ "${{GIT_SSH_COMMAND+x}}" != x ] && [ "${{GIT_SSH+x}}" != x ]; then
    if git -c core.fsmonitor=false -C "$directory" config --get core.sshCommand >/dev/null; then
        :
    else
        status=$?
        [ "$status" -eq 1 ] || exit "$status"
        export GIT_SSH_COMMAND='ssh -o BatchMode=yes -o StrictHostKeyChecking=yes'
    fi
fi
exec git -c core.fsmonitor=false -C "$directory" fetch --no-tags --quiet {} {}
"#,
        shell_quote(directory(path)?),
        shell_quote(item.remote.name()),
        shell_quote(&item.fetch_refspec()),
    ))
}

pub(super) fn run(
    host: &str,
    script: &str,
    deadline: Instant,
    cancelled: &impl Fn() -> bool,
) -> crate::Result<()> {
    let mut command = herdr_client::ssh::script_command(host, script)?;
    let (ok, _) = crate::pull_request::run(&mut command, deadline, cancelled)?;
    if !ok {
        return Err(herdr_vcs::Error::from(crate::git::Error::Failed {
            operation: "prepare the remote pull request or issue checkout",
            details:
                "The SSH command failed. Check the host's repository access and Git credentials."
                    .into(),
        })
        .into());
    }
    Ok(())
}
