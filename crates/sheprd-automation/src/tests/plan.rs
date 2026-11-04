use super::*;

pub(super) const NIGHTLY: &str = r#"
herdr.at("03:00", { tz = "Europe/Paris" }, function(fire)
  pull("main")
  local v = sh("just ci", { check = false })
  if not v.ok then
    agent {
      harness = "claude",
      worktree = "fix-validation-" .. fire.date,
      prompt = "Nightly validation failed on main:\n" .. v.log_tail .. "\nFix it.",
    }
  end
  agent { harness = "claude", worktree = "nightly-" .. fire.date, prompt = "Review main." }
end)
"#;

fn kinds(steps: &[PlannedStep]) -> Vec<Kind> {
    steps.iter().map(|step| step.call.step.kind()).collect()
}

#[test]
fn the_nightly_example_shows_every_branch() {
    let plan = plan(NIGHTLY);
    let steps = &plan.triggers.first().unwrap().steps;
    assert_eq!(kinds(steps), [Kind::Pull, Kind::Sh, Kind::Agent]);
    let Failure::Branch(instead) = &steps.get(1).unwrap().on_failure else {
        panic!("validation should branch: {steps:?}");
    };
    assert_eq!(kinds(instead), [Kind::Agent, Kind::Agent]);
    let Some(Step::Agent {
        worktree, prompt, ..
    }) = instead.first().map(|step| &step.call.step)
    else {
        panic!("the fix agent comes first");
    };
    assert_eq!(worktree.as_deref(), Some("fix-validation-{date}"));
    assert!(prompt.contains("{log_tail:2}"), "{prompt}");
    assert_eq!(steps.first().unwrap().on_failure, Failure::Stops);
    assert_eq!(steps.get(2).unwrap().on_failure, Failure::Stops);
}

#[test]
fn the_plan_reads_as_text() {
    let text = plan(NIGHTLY).to_string();
    let expected = "\
test
  caps: 2 agents per run · 2,000,000 tokens/day · 3h per run
  daily at 03:00 (Europe/Paris)
    1. pull main
    2. sh `just ci` (arbitrary command) · the script handles failure
       └ on failure, instead:
         1. agent claude in worktree fix-validation-{date}, prompt 55 chars
            ┆ prompt:
            ┆ Nightly validation failed on main:
            ┆ {log_tail:2}
            ┆ Fix it.
         2. agent claude in worktree nightly-{date}, prompt 12 chars
            ┆ prompt:
            ┆ Review main.
    3. agent claude in worktree nightly-{date}, prompt 12 chars
       ┆ prompt:
       ┆ Review main.
";
    assert_eq!(text, expected);
}

#[test]
fn a_long_or_multiline_command_is_shown_whole() {
    let text = plan(
        "herdr.manual(function()\n\
           sh(\"echo safe\\nrm -rf build\")\n\
           sh(\"short\")\n\
         end)",
    )
    .to_string();
    assert!(
        text.contains("1. sh `echo safe…` (arbitrary command)\n       ┆ command:\n       ┆ echo safe\n       ┆ rm -rf build\n"),
        "{text}"
    );
    assert!(
        text.contains("2. sh `short` (arbitrary command)\n"),
        "{text}"
    );
    assert!(!text.contains("┆ short"), "{text}");
}

#[test]
fn ignoring_a_failure_continues() {
    let plan =
        plan("herdr.manual(function() run(\"run\", { check = false }) notify(\"done\") end)");
    let steps = &plan.triggers.first().unwrap().steps;
    assert_eq!(steps.first().unwrap().on_failure, Failure::Continues);
}

#[test]
fn returning_on_failure_ends_the_run() {
    let plan = plan(
        "herdr.manual(function()\n\
           if not run(\"run\", { check = false }).ok then return end\n\
           notify(\"clean\")\n\
         end)",
    );
    let steps = &plan.triggers.first().unwrap().steps;
    assert_eq!(
        steps.first().unwrap().on_failure,
        Failure::Branch(Vec::new())
    );
    assert!(plan.to_string().contains("on failure: the run ends"));
}

#[test]
fn combined_failures_are_planned_too() {
    // Only both failing reaches the alarm; single failures would not show it.
    let plan = plan(
        "herdr.manual(function()\n\
           local a = sh(\"first\", { check = false })\n\
           local b = sh(\"second\", { check = false })\n\
           if not a.ok and not b.ok then notify(\"both failed\") end\n\
         end)",
    );
    let steps = &plan.triggers.first().unwrap().steps;
    let Failure::Branch(after_first) = &steps.first().unwrap().on_failure else {
        panic!("the first failure changes what follows: {steps:?}");
    };
    let Failure::Branch(after_both) = &after_first.first().unwrap().on_failure else {
        panic!("the second failure after the first branches: {after_first:?}");
    };
    assert_eq!(kinds(after_both), [Kind::Notify]);
    assert!(plan.to_string().contains("notify \"both failed\""));
}

#[test]
fn too_many_paths_are_refused() {
    let body = format!(
        "herdr.manual(function()\n{}end)",
        "sh(\"x\", { check = false })\n".repeat(9)
    );
    let error = Plan::build(&source(&body), Limits::default()).unwrap_err();
    assert!(
        matches!(error, Error::TooManyPaths { limit: MAX_PATHS }),
        "{error:?}"
    );
}

#[test]
fn a_manual_trigger_plans_by_name() {
    let plan = plan("herdr.manual({ name = \"deploy\" }, function() notify(\"go\") end)");
    assert!(plan.to_string().contains("deploy: manual only"));
}

#[test]
fn the_plan_carries_the_hash() {
    let source = source(NIGHTLY);
    assert_eq!(
        Plan::build(&source, Limits::default()).unwrap().hash,
        source.hash
    );
}

#[test]
fn the_plan_serializes_for_tools() {
    let value = serde_json::to_value(plan(NIGHTLY)).unwrap();
    assert_eq!(value["script"], "test");
    assert_eq!(value["caps"]["wall_time_seconds"], 10_800);
    let trigger = &value["triggers"][0];
    assert_eq!(trigger["schedule"]["kind"], "at");
    assert_eq!(trigger["zone"]["name"], "Europe/Paris");
    assert_eq!(trigger["steps"][0]["step"], "pull");
    assert_eq!(trigger["steps"][0]["branch"], "main");
    assert_eq!(trigger["steps"][0]["on_failure"]["then"], "stops");
    assert_eq!(trigger["steps"][1]["on_failure"]["then"], "branch");
    assert_eq!(
        trigger["steps"][1]["on_failure"]["steps"][0]["harness"],
        "claude"
    );
}
