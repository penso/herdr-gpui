use super::*;
use crate::{
    Profile, Tokens,
    pull_request::{
        batch::by_token,
        fetch::{Ask, Target},
    },
};
use secrecy::SecretString;

fn profile(login: &str, token: &Arc<SecretString>) -> Profile {
    Profile::fixture(login, token.clone(), &[])
}

fn target(owner: &str, branch: &str) -> (Input, Target) {
    (
        Input {
            checkout: None,
            repo_key: Some(format!("/{owner}/.git")),
            branch: branch.into(),
        },
        Target {
            owner: owner.into(),
            repo: "project".into(),
            ask: Ask::Number(1),
        },
    )
}

#[test]
fn each_account_asks_only_for_the_repositories_it_reads() {
    let main: Arc<SecretString> = Arc::new("main".into());
    let emu: Arc<SecretString> = Arc::new("emu".into());
    let (main_profile, emu_profile) = (profile("penso", &main), profile("fabien_microsoft", &emu));
    let tokens = Tokens::new(&main_profile, [(&emu_profile, &[][..])]);
    let groups = by_token(
        vec![
            target("rust-lang", "a"),
            target("fabien_microsoft", "b"),
            target("penso", "c"),
            target("Fabien_Microsoft", "d"),
        ],
        &tokens,
    );
    let shape: Vec<(bool, Vec<&str>)> = groups
        .iter()
        .map(|(_, token, targets)| {
            (
                Arc::ptr_eq(token, &main),
                targets
                    .iter()
                    .map(|(input, _)| input.branch.as_str())
                    .collect(),
            )
        })
        .collect();
    // In the order each account's first repository came.
    assert_eq!(shape, [(true, vec!["a", "c"]), (false, vec!["b", "d"])]);
    assert!(Arc::ptr_eq(&groups[1].1, &emu));
    // Each group names the account it is, which a pause is kept under.
    assert_eq!(groups[0].0, "penso");
    assert_eq!(groups[1].0, "fabien_microsoft");
}

#[test]
fn a_lone_token_asks_for_everything_in_one_group() {
    let token: Arc<SecretString> = Arc::new("lone".into());
    let groups = by_token(
        vec![target("one", "a"), target("two", "b")],
        &Tokens::from(token.clone()),
    );
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].2.len(), 2);
}

#[test]
fn adding_an_account_rescopes_the_cache_and_keeps_its_answers_apart() {
    let main: Arc<SecretString> = Arc::new("main".into());
    let emu: Arc<SecretString> = Arc::new("emu".into());
    let (main_profile, emu_profile) = (profile("penso", &main), profile("emu", &emu));
    let mut cache = Cache::default();
    let alone = Tokens::new(&main_profile, []);
    let key = alone.key();
    cache.scope((0, 1, "boot".into()), alone, &key, Origin::Local);
    assert_eq!(cache.view.login, "penso");
    let both = Tokens::new(&main_profile, [(&emu_profile, &[][..])]);
    let key = both.key();
    cache.scope((0, 1, "boot".into()), both.clone(), &key, Origin::Local);
    assert_eq!(cache.view.login, "penso,emu(emu)");
    assert!(cache.token.as_ref().unwrap().same(&both));
}
