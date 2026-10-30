use super::*;

fn tight() -> Limits {
    Limits {
        luau: herdr_luau::Limits {
            interrupts: 100_000,
            memory: 4 * 1024 * 1024,
        },
        nodes: 200,
    }
}

#[test]
fn a_runaway_load_view_or_update_stops() {
    let load_tight = |main: &str| Mod::load(&sources(main), tight(), SystemTime::UNIX_EPOCH);
    let error = load_tight("while true do end").unwrap_err();
    assert_eq!(error.budget(), Some(Budget::Instructions));

    let main = "local ui = require(\"@herdr/ui\")\n\
        return herdr.mod { update = function() while true do end end,\n\
          view = function() while true do end end }";
    let mut demo = load_tight(main).unwrap();
    let now = SystemTime::UNIX_EPOCH;
    let error = demo.view(&State::default(), now).unwrap_err();
    assert_eq!(error.budget(), Some(Budget::Instructions));
    let error = demo.update(&Data::Nil, now).unwrap_err();
    assert_eq!(error.budget(), Some(Budget::Instructions));
}

#[test]
fn pcall_cannot_swallow_the_budget() {
    let main = "pcall(function() while true do end end)\nwhile true do end";
    let error = Mod::load(&sources(main), tight(), SystemTime::UNIX_EPOCH).unwrap_err();
    assert_eq!(error.budget(), Some(Budget::Instructions));
}

#[test]
fn each_call_gets_the_whole_budget() {
    let main = format!(
        "local function work() local n = 0 for i = 1, 5000 do n += i end return n end\n{}",
        view_mod("(function() work() return ui.spacer() end)()")
    );
    let mut demo = Mod::load(&sources(&main), tight(), SystemTime::UNIX_EPOCH).unwrap();
    for _ in 0..50 {
        demo.view(&State::default(), SystemTime::UNIX_EPOCH)
            .unwrap();
    }
    assert!(demo.instructions() > 0);
}

#[test]
fn memory_is_bounded() {
    let main = "local t = {} for i = 1, 1e9 do t[i] = string.rep('x', 1024) end";
    let limits = Limits {
        luau: herdr_luau::Limits {
            interrupts: u64::MAX,
            ..tight().luau
        },
        ..tight()
    };
    let error = Mod::load(&sources(main), limits, SystemTime::UNIX_EPOCH).unwrap_err();
    assert_eq!(error.budget(), Some(Budget::Memory));
}
