use crate::actions::agent_name;

#[test]
fn agent_names_fit_herdr_limit_and_grammar() {
    let name = agent_name();
    assert!(
        (1..=32).contains(&name.len()),
        "Herdr allows 1-32 characters, got {}: {name}",
        name.len()
    );
    assert!(
        name.starts_with(|c: char| c.is_ascii_lowercase()),
        "{name} must start with a lowercase letter"
    );
    assert!(
        name.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '_')),
        "{name} must hold only lowercase letters, digits, '-' or '_'"
    );
}

#[test]
fn agent_names_differ_between_runs() {
    assert_ne!(agent_name(), agent_name());
}
