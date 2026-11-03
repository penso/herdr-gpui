use super::*;

#[test]
fn names_round_trip_and_are_unique() {
    for agent in Agent::ALL {
        assert_eq!(agent.name().parse::<Agent>(), Ok(*agent));
        assert_eq!(agent.to_string(), agent.name());
        assert!(!agent.display_name().is_empty());
        assert!(!agent.login_hint().is_empty());
        assert!(!agent.candidates().is_empty());
    }
    let mut names: Vec<_> = Agent::ALL.iter().map(|agent| agent.name()).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), Agent::ALL.len());
    assert_eq!("nope".parse::<Agent>(), Err(UnknownAgent));
    assert!(Agent::names().contains("claude"));
}
