use super::*;

#[test]
fn unsafe_and_nondeterministic_globals_are_gone() {
    let removed = [
        "os",
        "io",
        "debug",
        "require",
        "loadstring",
        "load",
        "dofile",
        "pcall",
        "xpcall",
        "print",
        "collectgarbage",
        "gcinfo",
        "getfenv",
        "setfenv",
        "newproxy",
        "coroutine",
        "math.random",
        "math.randomseed",
    ];
    let checks = removed
        .iter()
        .map(|name| format!("assert({name} == nil, \"{name} is still reachable\")\n"))
        .collect::<String>();
    load(&format!("{checks}herdr.manual(function() end)\n")).unwrap();
}

#[test]
fn the_safe_libraries_remain() {
    load(
        "assert(string.upper(\"a\") == \"A\")\n\
         assert(math.floor(1.5) == 1)\n\
         assert(table.concat({ \"a\", \"b\" }, \",\") == \"a,b\")\n\
         assert(utf8.len(\"é\") == 1)\n\
         herdr.manual(function() end)\n",
    )
    .unwrap();
}

#[test]
fn libraries_are_frozen() {
    let error = load("string.upper = nil\nherdr.manual(function() end)\n").unwrap_err();
    assert!(matches!(error, Error::Lua(_)), "{error:?}");
}

#[test]
fn steps_are_refused_at_the_top_level() {
    assert_eq!(
        refusal(load("pull(\"main\")\nherdr.manual(function() end)\n")),
        Refusal::StepOutsideTrigger("pull")
    );
}

#[test]
fn triggers_cannot_be_declared_while_running() {
    let script = load("herdr.manual(function() herdr.manual(function() end) end)\n").unwrap();
    let result = script.run::<Infallible>(0, |_| Verdict::Succeeded);
    assert_eq!(refusal(result), Refusal::TriggerInsideTrigger);
}

#[test]
fn every_step_has_a_global() {
    let checks = Kind::ALL
        .iter()
        .map(|kind| format!("assert(type({}) == \"function\")\n", kind.name()))
        .collect::<String>();
    load(&format!("{checks}herdr.manual(function() end)\n")).unwrap();
}

#[test]
fn step_names_are_unique() {
    let mut names = Kind::ALL.iter().map(|kind| kind.name()).collect::<Vec<_>>();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), Kind::ALL.len());
}

#[test]
fn a_script_sees_tokens_not_the_date() {
    let body = "herdr.manual(function(fire)\n\
                  if fire.date == \"2026-12-25\" then sh(\"rm -rf ~\") end\n\
                  notify(fire.date .. \" \" .. fire.time)\n\
                end)\n";
    let (calls, _) = record(body, 0);
    assert_eq!(
        calls
            .iter()
            .map(|call| call.step.clone())
            .collect::<Vec<_>>(),
        [Step::Notify {
            text: "{date} {time}".to_owned(),
        }]
    );
}

#[test]
fn no_state_carries_over_between_runs() {
    let body = "local runs = 0\n\
                herdr.manual(function()\n\
                  runs = runs + 1\n\
                  if runs > 1 then sh(\"second run\") end\n\
                  notify(tostring(runs))\n\
                end)\n";
    for _ in 0..3 {
        let (calls, _) = record(body, 0);
        assert_eq!(calls.len(), 1);
        assert_eq!(
            calls.first().map(|call| call.step.clone()),
            Some(Step::Notify {
                text: "1".to_owned(),
            })
        );
    }
}

#[test]
fn an_unknown_trigger_is_an_error() {
    let script = load("herdr.manual(function() end)\n").unwrap();
    let error = script
        .run::<Infallible>(3, |_| Verdict::Succeeded)
        .unwrap_err();
    assert!(matches!(error, Error::UnknownTrigger(3)));
}

#[test]
fn a_syntax_error_names_the_script() {
    let error = Source::new("nightly", "herdr.caps {")
        .load(Limits::default())
        .unwrap_err();
    assert!(error.to_string().contains("nightly"), "{error}");
}

#[test]
fn the_source_hash_pins_the_text() {
    let first = Source::new("a", "text");
    assert_eq!(first.hash.len(), 64);
    assert_eq!(first.hash, Source::new("b", "text").hash);
    assert_ne!(first.hash, Source::new("a", "text ").hash);
}
