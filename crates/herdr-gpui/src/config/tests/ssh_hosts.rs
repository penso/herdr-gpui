use super::*;
use herdr_client::HostKeys;

#[test]
fn hosts_skip_host_key_verification_only_when_listed() -> anyhow::Result<()> {
    let config = Config::parse("[ssh]\nskip_host_key_verification = ['dev.box.coder']")?;
    assert!(config.unknown_keys.is_empty(), "{:?}", config.unknown_keys);
    assert!(
        config
            .ssh
            .skip_host_key_verification
            .contains("dev.box.coder")
    );
    assert!(
        !config
            .ssh
            .skip_host_key_verification
            .contains("other.coder")
    );
    // Nothing is skipped by default.
    let config = Config::parse("")?;
    assert!(config.ssh.skip_host_key_verification.is_empty());
    assert!(
        !config
            .ssh
            .skip_host_key_verification
            .contains("dev.box.coder")
    );
    Ok(())
}

#[test]
fn a_list_that_names_no_ssh_target_is_refused() -> anyhow::Result<()> {
    for text in [
        "[ssh]\nskip_host_key_verification = ['-oProxyCommand=x']",
        "[ssh]\nskip_host_key_verification = ['two words']",
        "[ssh]\nskip_host_key_verification = ['']",
    ] {
        let error = Config::parse(text).err().context("accepted")?;
        assert!(
            matches!(
                error,
                crate::Error::SshHosts(crate::config::ssh_hosts::Error::InvalidTarget(_))
            ),
            "{text:?} gave {error:?}"
        );
    }
    let many = (0..=herdr_client::MAX_UNVERIFIED_HOSTS)
        .map(|index| format!("'host-{index}'"))
        .collect::<Vec<_>>()
        .join(", ");
    let error = Config::parse(&format!("[ssh]\nskip_host_key_verification = [{many}]"))
        .err()
        .context("accepted")?;
    assert!(
        matches!(
            error,
            crate::Error::SshHosts(crate::config::ssh_hosts::Error::TooMany(_))
        ),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn saving_a_choice_edits_only_the_ssh_list() -> anyhow::Result<()> {
    let temp = TempDirectory::new()?;
    let path = temp.0.join("config.toml");
    let original = "theme = 'Nord' # keep\n";
    fs::write(&path, original)?;
    crate::config::ssh_hosts::save_host_keys_path("dev.box.coder", HostKeys::Skip, &path)?;
    crate::config::ssh_hosts::save_host_keys_path("other.coder", HostKeys::Skip, &path)?;
    // Saving the same choice again changes nothing.
    let saved = fs::read_to_string(&path)?;
    crate::config::ssh_hosts::save_host_keys_path("dev.box.coder", HostKeys::Skip, &path)?;
    assert_eq!(fs::read_to_string(&path)?, saved);
    assert!(saved.starts_with(original), "{saved}");
    let config = Config::parse(&saved)?;
    assert!(
        config
            .ssh
            .skip_host_key_verification
            .contains("dev.box.coder")
    );
    assert!(
        config
            .ssh
            .skip_host_key_verification
            .contains("other.coder")
    );
    assert_eq!(config.theme, "Nord");

    // Verifying again removes the target, and the table once it is empty.
    crate::config::ssh_hosts::save_host_keys_path("dev.box.coder", HostKeys::Verify, &path)?;
    let config = Config::parse(&fs::read_to_string(&path)?)?;
    assert!(
        !config
            .ssh
            .skip_host_key_verification
            .contains("dev.box.coder")
    );
    assert!(
        config
            .ssh
            .skip_host_key_verification
            .contains("other.coder")
    );
    crate::config::ssh_hosts::save_host_keys_path("other.coder", HostKeys::Verify, &path)?;
    assert_eq!(fs::read_to_string(&path)?, original);
    // Verifying a host that was never listed leaves the file alone.
    crate::config::ssh_hosts::save_host_keys_path("never.coder", HostKeys::Verify, &path)?;
    assert_eq!(fs::read_to_string(&path)?, original);

    // A target that could never load never reaches the file.
    assert!(matches!(
        crate::config::ssh_hosts::save_host_keys_path("-oProxyCommand=x", HostKeys::Skip, &path),
        Err(crate::Error::SshHosts(
            crate::config::ssh_hosts::Error::InvalidTarget(_)
        ))
    ));
    assert_eq!(fs::read_to_string(&path)?, original);
    Ok(())
}
