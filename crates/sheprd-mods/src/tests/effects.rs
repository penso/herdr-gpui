use super::*;

fn effect_problem(effects: &str) -> (String, Problem) {
    let main = format!(
        "local ui = require(\"@herdr/ui\")\n\
         return herdr.mod {{ update = function(state) return state, {effects} end,\n\
           view = function() return ui.spacer() end }}"
    );
    match load(&main)
        .unwrap()
        .update(&Data::Nil, SystemTime::UNIX_EPOCH)
        .unwrap_err()
    {
        Error::Effect { at, problem } => (at, problem),
        other => panic!("expected an effect error, got {other:?}"),
    }
}

#[test]
fn timers_are_bounded() {
    assert_eq!(
        effect_problem("{ herdr.timer(0.1, \"x\") }").0,
        "effects[1].after"
    );
    assert_eq!(
        effect_problem("{ herdr.timer(1e9, \"x\") }").0,
        "effects[1].after"
    );
}

#[test]
fn effects_are_a_short_list_of_known_kinds() {
    assert_eq!(effect_problem("\"run\"").0, "effects");
    assert_eq!(effect_problem("{ { effect = \"exec\" } }").0, "effects[1]");
    assert_eq!(
        effect_problem("{ { effect = \"run\", command = \"new_window\", extra = 1 } }").1,
        Problem::Unknown("extra".to_owned())
    );
    let many = vec!["herdr.run(\"new_window\")"; 17].join(", ");
    assert_eq!(
        effect_problem(&format!("{{ {many} }}")).1,
        Problem::Entries { limit: 16 }
    );
}

#[test]
fn run_checks_the_command_when_called() {
    let main = format!("herdr.run(\"rm_rf\")\n{}", view_mod("ui.spacer()"));
    let error = load(&main).unwrap_err();
    assert!(
        format!("{error}").contains("unknown command `rm_rf`"),
        "{error}"
    );
}
