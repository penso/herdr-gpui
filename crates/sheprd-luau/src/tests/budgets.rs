use super::*;

fn tight() -> Limits {
    Limits {
        interrupts: 10_000,
        memory: 4 * 1024 * 1024,
    }
}

#[test]
fn a_loop_runs_out_of_instructions() {
    let (lua, _) = sealed(Policy::Deterministic, tight());
    let error = lua.load("while true do end").exec().unwrap_err();
    assert_eq!(
        error.downcast_ref::<OutOfInstructions>(),
        Some(&OutOfInstructions)
    );
}

#[test]
fn reset_gives_the_next_run_the_whole_budget() {
    let (lua, meter) = sealed(Policy::Deterministic, tight());
    let busy = lua
        .load("return function() local n = 0 for i = 1, 500 do n += i end return n end")
        .eval::<mlua::Function>()
        .unwrap();
    for _ in 0..100 {
        meter.reset();
        busy.call::<i64>(()).unwrap();
    }
    assert!(meter.used() > 0);
}

#[test]
fn growing_past_the_memory_limit_fails() {
    let (lua, _) = sealed(
        Policy::Interactive,
        Limits {
            interrupts: u64::MAX,
            ..tight()
        },
    );
    let error = lua
        .load("local t = {} for i = 1, 1e9 do t[i] = string.rep('x', 1024) end")
        .exec()
        .unwrap_err();
    assert!(matches!(error, mlua::Error::MemoryError(_)), "{error:?}");
}
