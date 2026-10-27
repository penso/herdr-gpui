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
    Ok(format!(
        "export GIT_TERMINAL_PROMPT=0 GCM_INTERACTIVE=never; export GIT_SSH_COMMAND='ssh -o BatchMode=yes -o StrictHostKeyChecking=yes'; exec git -c core.fsmonitor=false -C {} fetch --no-tags --quiet {} {}",
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
