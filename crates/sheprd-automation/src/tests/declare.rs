use super::*;

fn invalid(problem: Problem, function: &'static str) -> Refusal {
    Refusal::Invalid { function, problem }
}

#[test]
fn caps_are_required() {
    let error =
        Script::load("test", "herdr.manual(function() end)", Limits::default()).unwrap_err();
    assert!(matches!(error, Error::MissingCaps));
}

#[test]
fn a_trigger_is_required() {
    assert!(matches!(load("").unwrap_err(), Error::NoTriggers));
}

#[test]
fn caps_are_declared_once() {
    assert_eq!(
        refusal(load(&format!("{CAPS}herdr.manual(function() end)"))),
        Refusal::CapsTwice
    );
}

#[test]
fn caps_are_read_and_bounded() {
    let script = load("herdr.manual(function() end)").unwrap();
    assert_eq!(
        *script.caps(),
        Caps {
            agents_per_run: 2,
            tokens_per_day: 2_000_000,
            wall_time: std::time::Duration::from_secs(3 * 3_600),
        }
    );
    let missing = Script::load(
        "test",
        "herdr.caps { agents_per_run = 1, tokens_per_day = 10 }\nherdr.manual(function() end)",
        Limits::default(),
    );
    assert_eq!(
        refusal(missing),
        invalid(Problem::Missing("wall_time"), "herdr.caps")
    );
    let range = Script::load(
        "test",
        "herdr.caps { agents_per_run = 99, tokens_per_day = 10, wall_time = \"1h\" }\nherdr.manual(function() end)",
        Limits::default(),
    );
    assert!(matches!(
        refusal(range),
        Refusal::Invalid {
            problem: Problem::Range {
                field: "agents_per_run",
                ..
            },
            ..
        }
    ));
}

#[test]
fn at_reads_time_days_zone_and_name() {
    let script = load("herdr.at(\"03:00\", { days = \"mon-fri\", tz = \"Europe/Paris\", name = \"nightly\" }, function() end)").unwrap();
    let trigger = script.triggers().first().unwrap();
    assert_eq!(trigger.name.as_deref(), Some("nightly"));
    assert_eq!(
        trigger.schedule,
        Schedule::At {
            time: ClockTime { hour: 3, minute: 0 },
            days: Days::WEEKDAYS,
        }
    );
    assert_eq!(trigger.zone, Zone::Named("Europe/Paris".to_owned()));
    assert_eq!(
        trigger.to_string(),
        "nightly: weekdays at 03:00 (Europe/Paris)"
    );
}

#[test]
fn at_refuses_bad_values() {
    assert_eq!(
        refusal(load("herdr.at(\"3am\", function() end)")),
        invalid(Problem::Time("3am".to_owned()), "herdr.at")
    );
    assert_eq!(
        refusal(load(
            "herdr.at(\"03:00\", { days = \"someday\" }, function() end)"
        )),
        invalid(Problem::Days("someday".to_owned()), "herdr.at")
    );
    assert_eq!(
        refusal(load(
            "herdr.at(\"03:00\", { tz = \"Mars/Olympus\" }, function() end)"
        )),
        invalid(Problem::TimeZone("Mars/Olympus".to_owned()), "herdr.at")
    );
    assert_eq!(
        refusal(load(
            "herdr.at(\"03:00\", { when = \"now\" }, function() end)"
        )),
        invalid(Problem::Unknown("when".to_owned()), "herdr.at")
    );
    assert!(matches!(
        refusal(load("herdr.at(\"03:00\")")),
        Refusal::Invalid {
            problem: Problem::Type {
                field: "handler",
                ..
            },
            ..
        }
    ));
}

#[test]
fn every_and_cron_are_bounded() {
    let script = load("herdr.every(\"6h\", function() end)\nherdr.cron(\"*/15 9-17 * * mon-fri\", function() end)").unwrap();
    let schedules = script
        .triggers()
        .iter()
        .map(|trigger| trigger.schedule.to_string())
        .collect::<Vec<_>>();
    assert_eq!(schedules, ["every 6h", "cron */15 9-17 * * mon-fri"]);
    assert!(matches!(
        refusal(load("herdr.every(\"30s\", function() end)")),
        Refusal::Invalid {
            problem: Problem::Range {
                field: "interval",
                ..
            },
            ..
        }
    ));
    assert!(matches!(
        refusal(load("herdr.cron(\"* * *\", function() end)")),
        Refusal::Invalid {
            problem: Problem::Cron { .. },
            ..
        }
    ));
}

#[test]
fn manual_triggers_have_no_schedule() {
    let script =
        load("herdr.manual(function() end)\nherdr.manual({ name = \"deploy\" }, function() end)")
            .unwrap();
    let triggers = script.triggers();
    assert!(
        triggers
            .iter()
            .all(|trigger| trigger.schedule == Schedule::Manual)
    );
    assert_eq!(
        triggers.get(1).and_then(|trigger| trigger.name.as_deref()),
        Some("deploy")
    );
    assert_eq!(triggers.get(1).unwrap().to_string(), "deploy: manual only");
}

#[test]
fn trigger_names_are_unique() {
    assert!(matches!(
        refusal(load(
            "herdr.manual({ name = \"a\" }, function() end)\nherdr.every(\"1h\", { name = \"a\" }, function() end)"
        )),
        Refusal::Invalid {
            problem: Problem::Range { field: "name", .. },
            ..
        }
    ));
}

#[test]
fn triggers_per_script_are_capped() {
    let many = "herdr.manual(function() end)\n".repeat(17);
    assert!(matches!(
        refusal(load(&many)),
        Refusal::Invalid {
            problem: Problem::Range {
                field: "triggers",
                ..
            },
            ..
        }
    ));
}
