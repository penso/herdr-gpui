use super::*;
use crate::{
    config::{GitHubAccount, MAX_ACCOUNTS, valid_account_id},
    github::{self, AccountsError},
};

const ID: &str = "0123456789abcdef0123456789abcdef";
const OTHER: &str = "fedcba9876543210fedcba9876543210";

fn accounts_error(text: &str) -> Option<AccountsError> {
    match Config::parse(text) {
        Err(crate::Error::GitHub(github::Error::Accounts(error))) => Some(error),
        _ => None,
    }
}

#[test]
fn accounts_load_in_order_with_their_owners() -> anyhow::Result<()> {
    assert!(Config::default().github.accounts.is_empty());
    let config = Config::parse(&format!(
        "[github]\naccounts = [{{ id = \"{ID}\", owners = [\"emu-org\", \"name_shortcode\"] }}, {{ id = \"{OTHER}\" }}]"
    ))?;
    assert_eq!(
        config.github.accounts,
        [
            GitHubAccount {
                id: ID.into(),
                owners: vec!["emu-org".into(), "name_shortcode".into()],
            },
            GitHubAccount {
                id: OTHER.into(),
                owners: Vec::new(),
            },
        ]
    );
    Ok(())
}

#[test]
fn accounts_the_app_could_not_tell_apart_or_store_are_refused() {
    assert!(matches!(
        accounts_error("[github]\naccounts = [{ id = \"../escape\" }]"),
        Some(AccountsError::Id)
    ));
    assert!(matches!(
        accounts_error(&format!(
            "[github]\naccounts = [{{ id = \"{}\" }}]",
            ID.to_uppercase()
        )),
        Some(AccountsError::Id)
    ));
    assert!(matches!(
        accounts_error(&format!(
            "[github]\naccounts = [{{ id = \"{ID}\" }}, {{ id = \"{ID}\" }}]"
        )),
        Some(AccountsError::DuplicateId(id)) if id == ID
    ));
    assert!(matches!(
        accounts_error(&format!(
            "[github]\naccounts = [{{ id = \"{ID}\", owners = [\"not an owner\"] }}]"
        )),
        Some(AccountsError::Owner(owner)) if owner == "not an owner"
    ));
    // A token pasted into an account entry is refused, not silently kept.
    assert!(matches!(
        Config::parse(&format!(
            "[github]\naccounts = [{{ id = \"{ID}\", token = \"ghp_secret\" }}]"
        )),
        Err(crate::Error::GitHub(github::Error::SecretInConfig("token")))
    ));
}

#[test]
fn what_a_newer_build_adds_is_ignored_and_named_not_refused() -> anyhow::Result<()> {
    // More accounts and owners than this build keeps.
    let many: Vec<String> = (0..MAX_ACCOUNTS + 2)
        .map(|index| format!("{{ id = \"{index:032x}\" }}"))
        .collect();
    let config = Config::parse(&format!("[github]\naccounts = [{}]", many.join(", ")))?;
    assert_eq!(config.github.accounts.len(), MAX_ACCOUNTS);
    assert!(
        config
            .unknown_keys
            .contains(&format!("github.accounts.{MAX_ACCOUNTS}")),
        "{:?}",
        config.unknown_keys
    );
    let owners: Vec<String> = (0..40).map(|index| format!("\"owner-{index}\"")).collect();
    let config = Config::parse(&format!(
        "[github]\naccounts = [{{ id = \"{ID}\", owners = [{}] }}]",
        owners.join(", ")
    ))?;
    assert_eq!(config.github.accounts[0].owners.len(), 32);
    assert!(
        config
            .unknown_keys
            .contains(&"github.accounts.0.owners.32".to_owned())
    );
    // A field this build does not know.
    let config = Config::parse(&format!(
        "[github]\naccounts = [{{ id = \"{ID}\", label = \"Work\" }}]"
    ))?;
    assert_eq!(config.github.accounts[0].id, ID);
    assert!(
        config
            .unknown_keys
            .iter()
            .any(|key| key.starts_with("github.accounts") && key.ends_with("label")),
        "{:?}",
        config.unknown_keys
    );
    Ok(())
}

#[test]
fn generated_ids_are_valid_and_distinct() -> anyhow::Result<()> {
    let first = GitHubAccount::generate()?;
    let second = GitHubAccount::generate()?;
    assert!(valid_account_id(&first.id));
    assert_ne!(first.id, second.id);
    assert!(first.owners.is_empty());
    Ok(())
}

fn files(text: &str) -> anyhow::Result<(TempDirectory, PathBuf, PathBuf, PathBuf)> {
    let temp = TempDirectory::new()?;
    let path = temp.0.join("config-gpui.toml");
    let local = path.with_extension("local.toml");
    let daemon = temp.0.join("absent.toml");
    fs::write(&local, text)?;
    Ok((temp, path, local, daemon))
}

fn account(id: &str, owners: &[&str]) -> GitHubAccount {
    GitHubAccount {
        id: id.into(),
        owners: owners.iter().map(|owner| (*owner).to_owned()).collect(),
    }
}

#[test]
fn adding_and_removing_accounts_keeps_the_rest_of_the_file() -> anyhow::Result<()> {
    let (_temp, path, local, daemon) = files(
        "# Mine\ntheme = \"Nord\" # kept\n\n[github]\nallow_plaintext_credentials = false\n",
    )?;
    Config::add_github_account_at(&local, &daemon, &account(ID, &["emu-org"]))?;
    Config::add_github_account_at(&local, &daemon, &account(OTHER, &[]))?;
    let text = fs::read_to_string(&local)?;
    assert!(
        text.starts_with("# Mine\ntheme = \"Nord\" # kept\n"),
        "{text}"
    );
    assert!(
        text.contains("allow_plaintext_credentials = false"),
        "{text}"
    );
    let loaded = Config::check_files_at(&path, &daemon)?;
    assert_eq!(
        loaded.github.accounts,
        [account(ID, &["emu-org"]), account(OTHER, &[])]
    );
    Config::remove_github_account_at(&local, &daemon, ID)?;
    let loaded = Config::check_files_at(&path, &daemon)?;
    assert_eq!(loaded.github.accounts, [account(OTHER, &[])]);
    // The last one gone takes the key with it.
    Config::remove_github_account_at(&local, &daemon, OTHER)?;
    let text = fs::read_to_string(&local)?;
    assert!(!text.contains("accounts"), "{text}");
    assert!(
        text.contains("allow_plaintext_credentials = false"),
        "{text}"
    );
    Ok(())
}

#[test]
fn editing_keeps_entries_as_a_newer_build_or_a_person_wrote_them() -> anyhow::Result<()> {
    let (_temp, path, local, daemon) = files(&format!(
        "[[github.accounts]]\nid = \"{ID}\"\nlabel = \"Work\"\nowners = [\"emu-org\"]\n"
    ))?;
    Config::add_github_account_at(&local, &daemon, &account(OTHER, &[]))?;
    let text = fs::read_to_string(&local)?;
    assert!(text.contains("label = \"Work\""), "{text}");
    let loaded = Config::check_files_at(&path, &daemon)?;
    assert_eq!(
        loaded.github.accounts,
        [account(ID, &["emu-org"]), account(OTHER, &[])]
    );
    Config::remove_github_account_at(&local, &daemon, OTHER)?;
    assert!(fs::read_to_string(&local)?.contains("label = \"Work\""));
    // Removing an account that is not listed changes nothing.
    let before = fs::read_to_string(&local)?;
    Config::remove_github_account_at(&local, &daemon, OTHER)?;
    assert_eq!(fs::read_to_string(&local)?, before);
    Ok(())
}

#[test]
fn an_add_the_app_would_refuse_is_not_written() -> anyhow::Result<()> {
    let (_temp, _path, local, daemon) = files("theme = \"Nord\"\n")?;
    assert!(matches!(
        Config::add_github_account_at(&local, &daemon, &account("bad", &[])),
        Err(crate::Error::GitHub(github::Error::Accounts(
            AccountsError::Id
        )))
    ));
    assert!(matches!(
        Config::add_github_account_at(&local, &daemon, &account(ID, &["not an owner"])),
        Err(crate::Error::GitHub(github::Error::Accounts(
            AccountsError::Owner(_)
        )))
    ));
    assert_eq!(fs::read_to_string(&local)?, "theme = \"Nord\"\n");
    for index in 0..MAX_ACCOUNTS {
        Config::add_github_account_at(&local, &daemon, &account(&format!("{index:032x}"), &[]))?;
    }
    assert!(matches!(
        Config::add_github_account_at(&local, &daemon, &account(ID, &[])),
        Err(crate::Error::GitHub(github::Error::Accounts(
            AccountsError::TooMany(_)
        )))
    ));
    Ok(())
}
