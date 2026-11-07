use super::*;

#[test]
fn the_stat_script_reads_only_the_pid_it_names() {
    let script = STAT_SCRIPT.replace("{pid}", "42");
    assert_eq!(
        script,
        "printf '\\036%s\\n' 42\nif [ -r /proc/42/stat ]; then\n  IFS= read -r stat < /proc/42/stat || :\n  fields=${stat##*) }\n  set -- $fields\n  if [ \"$#\" -ge 20 ]; then printf '%s %s %s\\n' \"$2\" \"$3\" \"${20}\"; fi\nfi\n"
    );
}

#[test]
fn only_codex_and_claude_conversations_are_observable() {
    for name in ["", "Codex", "bash"] {
        assert_eq!(observable(name), None, "{name}");
    }
    for agent in Agent::ALL {
        let expected = matches!(agent, Agent::Codex | Agent::Claude).then_some(*agent);
        assert_eq!(observable(agent.name()), expected);
    }
}
