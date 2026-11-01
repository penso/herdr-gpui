//! Azure DevOps tokens pasted in the account menu: kept per organization in
//! one record, read back in the worker's lookup order, and never kept in a
//! form the store would refuse to read.
use crate::azure::{
    self,
    saved::{MAX_ORGANIZATIONS, Saved, normalize_organization, read_in, save_in, token_in},
    token::{self, Source},
};
use herdr_credentials::Store;
use secrecy::{ExposeSecret, SecretString};
use std::{ffi::OsString, sync::Arc};

fn secret(text: &str) -> SecretString {
    SecretString::from(text)
}

#[test]
fn an_organization_is_kept_by_its_name_whatever_address_was_pasted() {
    for typed in [
        "contoso",
        "Contoso",
        " contoso ",
        "https://dev.azure.com/contoso",
        "https://dev.azure.com/Contoso/",
        "dev.azure.com/contoso/Fabrikam/_git/web",
        "https://contoso.visualstudio.com",
        "contoso.visualstudio.com/DefaultCollection",
    ] {
        assert_eq!(
            normalize_organization(typed).as_deref(),
            Some("contoso"),
            "{typed}"
        );
    }
    for refused in [
        "",
        "-contoso",
        "contoso-",
        "two words",
        "https://example.test/contoso",
        "ftp://dev.azure.com/contoso",
        &"a".repeat(51),
    ] {
        assert_eq!(normalize_organization(refused), None, "{refused}");
    }
}

#[test]
fn saved_tokens_read_back_by_organization_and_forget_one_at_a_time() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path();
    assert_eq!(
        read_in(Store::File, path).unwrap().organizations().count(),
        0
    );
    save_in(Store::File, path, "Contoso", Some(secret(" pat-one \n"))).unwrap();
    save_in(Store::File, path, "fabrikam", Some(secret("pat-two"))).unwrap();
    let saved = read_in(Store::File, path).unwrap();
    assert_eq!(
        saved.organizations().collect::<Vec<_>>(),
        ["contoso", "fabrikam"]
    );
    assert_eq!(saved.get("CONTOSO").unwrap().expose_secret(), "pat-one");
    assert_eq!(
        token_in(Store::File, path, "fabrikam")
            .unwrap()
            .expose_secret(),
        "pat-two"
    );
    save_in(Store::File, path, "contoso", None).unwrap();
    let saved = read_in(Store::File, path).unwrap();
    assert_eq!(saved.organizations().collect::<Vec<_>>(), ["fabrikam"]);
    save_in(Store::File, path, "fabrikam", None).unwrap();
    // The last forget removes the file rather than leaving an empty record.
    assert_eq!(std::fs::read_dir(path).unwrap().count(), 0);
}

#[test]
fn a_bad_organization_or_token_is_refused_before_it_is_saved() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path();
    assert!(matches!(
        save_in(Store::File, path, "two words", Some(secret("pat"))),
        Err(azure::Error::Organization(_))
    ));
    assert!(matches!(
        save_in(Store::File, path, "contoso", Some(secret("two words"))),
        Err(azure::Error::Token)
    ));
    assert_eq!(std::fs::read_dir(path).unwrap().count(), 0);
}

#[test]
fn the_record_holds_a_bounded_number_of_organizations() {
    let mut saved = Saved::default();
    for index in 0..MAX_ORGANIZATIONS {
        saved
            .set(&format!("org{index}"), Some(secret("pat")))
            .unwrap();
    }
    assert!(matches!(
        saved.set("one-more", Some(secret("pat"))),
        Err(azure::Error::TooManyOrganizations)
    ));
    // Replacing a kept organization's token is not one more.
    saved.set("org0", Some(secret("pat-new"))).unwrap();
    let record = (0..=MAX_ORGANIZATIONS)
        .map(|index| format!("org{index}\tpat\n"))
        .collect::<String>();
    assert!(matches!(
        Saved::parse(&secret(&record)),
        Err(azure::Error::Record)
    ));
}

#[test]
fn a_malformed_record_is_refused_unread_and_replaced_by_the_next_save() {
    for record in ["contoso", "Contoso\tpat\n", "contoso\ttwo words\n"] {
        assert!(
            matches!(Saved::parse(&secret(record)), Err(azure::Error::Record)),
            "{record:?}"
        );
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path();
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path.join("azure-devops-credentials"))
            .unwrap();
        std::io::Write::write_all(&mut file, b"not a record\n").unwrap();
    }
    assert!(matches!(
        read_in(Store::File, path),
        Err(azure::Error::Record)
    ));
    // The worker skips an unreadable record rather than failing the sync.
    assert!(token_in(Store::File, path, "contoso").is_none());
    save_in(Store::File, path, "contoso", Some(secret("pat-new"))).unwrap();
    assert_eq!(
        read_in(Store::File, path)
            .unwrap()
            .get("contoso")
            .unwrap()
            .expose_secret(),
        "pat-new"
    );
}

#[test]
fn the_environment_beats_a_saved_token_which_beats_the_cli() {
    let asked = Arc::new(std::sync::Mutex::new(Vec::new()));
    let resolve = |environment: Option<&str>, saved: Option<&str>, cli: Option<&str>| {
        let environment = environment.map(OsString::from);
        let (saved_asked, cli_asked) = (asked.clone(), asked.clone());
        token::resolve(
            move |name| {
                (name == token::PAT_VARIABLE)
                    .then(|| environment.clone())
                    .flatten()
            },
            move || {
                saved_asked.lock().unwrap().push("saved");
                saved.map(secret)
            },
            move || {
                cli_asked.lock().unwrap().push("cli");
                cli.map(|token| token::Token::Entra(secret(token)))
            },
        )
        .map(|found| found.source)
    };
    assert_eq!(
        resolve(Some("pat-env"), Some("pat-saved"), Some("cli")),
        Some(Source::Environment)
    );
    // An earlier source with a token leaves the later ones unasked.
    assert!(asked.lock().unwrap().is_empty());
    assert_eq!(
        resolve(None, Some("pat-saved"), Some("cli")),
        Some(Source::Saved)
    );
    assert_eq!(*asked.lock().unwrap(), ["saved"]);
    assert_eq!(resolve(None, None, Some("cli")), Some(Source::Cli));
    assert_eq!(resolve(None, None, None), None);
}

#[test]
fn only_a_missing_or_refused_token_offers_the_sign_in() {
    let repository = || "contoso/project/web".to_owned();
    assert!(
        azure::Error::SignIn {
            repository: repository()
        }
        .needs_sign_in()
    );
    assert!(azure::Error::Authentication.needs_sign_in());
    assert!(
        !azure::Error::Forbidden {
            repository: repository()
        }
        .needs_sign_in()
    );
    assert!(!azure::Error::RateLimit.needs_sign_in());
    // A refused AZURE_DEVOPS_EXT_PAT is not fixed by a token saved in the
    // app, which is only tried after it.
    let refused = azure::Error::Authentication.from_source(Source::Environment);
    assert!(matches!(refused, azure::Error::EnvironmentRefused));
    assert!(!refused.needs_sign_in());
    assert!(refused.needs_user());
    assert!(matches!(
        azure::Error::Authentication.from_source(Source::Saved),
        azure::Error::Authentication
    ));
}

#[test]
fn no_credential_store_says_how_to_opt_in_for_azure_devops() {
    let error = azure::Error::from(herdr_credentials::Error::Policy);
    assert!(matches!(error, azure::Error::NoStore(_)));
    assert!(
        error
            .to_string()
            .contains("[azure_devops] allow_plaintext_credentials")
    );
    assert!(std::error::Error::source(&error).is_some());
    let config = azure::Config::default();
    assert_eq!(config.store(), Store::for_policy(false));
    let opted = azure::Config {
        allow_plaintext_credentials: true,
    };
    assert_eq!(opted.store(), Store::for_policy(true));
}
