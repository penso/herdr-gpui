use super::*;

fn profile(login: &str, token: &str, organizations: &[&str]) -> Profile {
    Profile::fixture(login, Arc::new(token.into()), organizations)
}

fn exposed(token: &Arc<SecretString>) -> &str {
    secrecy::ExposeSecret::expose_secret(&**token)
}

#[test]
fn an_owner_no_additional_account_covers_is_read_with_the_main_account() {
    let main = profile("penso", "main-token", &[]);
    let emu = profile("fabien_microsoft", "emu-token", &["emu-org"]);
    let tokens = Tokens::new(&main, [(&emu, &[][..])]);
    assert_eq!(exposed(tokens.for_owner("rust-lang")), "main-token");
    assert_eq!(tokens.login_for("rust-lang"), "penso");
    // The main account's own repositories are the main account's.
    assert_eq!(exposed(tokens.for_owner("penso")), "main-token");
}

#[test]
fn an_additional_account_reads_its_login_organizations_and_listed_owners() {
    let main = profile("penso", "main-token", &[]);
    let emu = profile("fabien_microsoft", "emu-token", &["emu-org"]);
    let owners = vec!["listed-owner".to_owned()];
    let tokens = Tokens::new(&main, [(&emu, owners.as_slice())]);
    for owner in [
        "fabien_microsoft",
        "FABIEN_MICROSOFT",
        "emu-org",
        "Listed-Owner",
    ] {
        assert_eq!(exposed(tokens.for_owner(owner)), "emu-token", "{owner}");
        assert_eq!(tokens.login_for(owner), "fabien_microsoft");
    }
}

#[test]
fn the_first_listed_account_covering_an_owner_wins() {
    let main = profile("penso", "main-token", &[]);
    let first = profile("first", "first-token", &["shared"]);
    let second = profile("second", "second-token", &["shared"]);
    let tokens = Tokens::new(&main, [(&first, &[][..]), (&second, &[][..])]);
    assert_eq!(exposed(tokens.for_owner("shared")), "first-token");
    assert_eq!(exposed(tokens.for_owner("second")), "second-token");
}

#[test]
fn sameness_follows_token_allocations_and_coverage_not_token_text() {
    let main = profile("penso", "main-token", &[]);
    let emu = profile("emu", "emu-token", &[]);
    let tokens = Tokens::new(&main, [(&emu, &[][..])]);
    assert!(tokens.same(&tokens.clone()));
    assert!(tokens.same(&Tokens::new(&main, [(&emu, &[][..])])));
    // A renewed token is a new allocation, even with the same text.
    let renewed = profile("emu", "emu-token", &[]);
    assert!(!tokens.same(&Tokens::new(&main, [(&renewed, &[][..])])));
    let owners = vec!["more".to_owned()];
    assert!(!tokens.same(&Tokens::new(&main, [(&emu, owners.as_slice())])));
    assert!(!tokens.same(&Tokens::new(&main, [])));
}

#[test]
fn the_key_names_every_account_main_first_with_the_owners_it_reads() {
    let main = profile("penso", "main-token", &[]);
    let emu = profile("emu", "emu-token", &["emu-org"]);
    assert_eq!(
        Tokens::new(&main, [(&emu, &[][..])]).key(),
        "penso,emu(emu emu-org)"
    );
    assert_eq!(Tokens::new(&main, []).key(), "penso");
    // Moving an owner to an account is another set, whose answers are its own.
    let owners = vec!["acme".to_owned()];
    assert_eq!(
        Tokens::new(&main, [(&emu, owners.as_slice())]).key(),
        "penso,emu(emu emu-org acme)"
    );
    let tokens = Tokens::new(&main, [(&emu, &[][..])]);
    assert_eq!(tokens.logins().collect::<Vec<_>>(), ["penso", "emu"]);
}

#[test]
fn a_lone_token_reads_every_owner() {
    let token: Arc<SecretString> = Arc::new("lone".into());
    let tokens = Tokens::from(token.clone());
    assert!(Arc::ptr_eq(tokens.for_owner("anyone"), &token));
}
