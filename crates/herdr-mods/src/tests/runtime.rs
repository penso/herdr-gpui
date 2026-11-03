use super::*;

const COUNTER: &str = "local ui = require(\"@herdr/ui\")\n\
    return herdr.mod {\n\
      watch = { \"agents\", \"agents\" },\n\
      init = function() return { count = 0 } end,\n\
      update = function(state, msg)\n\
        if msg == \"add\" then state.count += 1 end\n\
        if msg == \"keep\" then return nil end\n\
        if msg == \"later\" then return state, { herdr.timer(5, \"add\"), herdr.run(\"new_window\") } end\n\
        herdr.log(\"got\", msg, state.count)\n\
        return state\n\
      end,\n\
      view = function(state, app)\n\
        return ui.text(state.count .. \"/\" .. #app.agents.blocked)\n\
      end,\n\
    }\n";

fn agents() -> State {
    let agent = |id: &str, status| Agent {
        id: id.to_owned(),
        host: "local".to_owned(),
        workspace: "w1".to_owned(),
        name: "claude".to_owned(),
        status,
    };
    State {
        agents: vec![
            agent("p1", AgentStatus::Blocked),
            agent("p2", AgentStatus::Working),
            agent("p3", AgentStatus::Blocked),
        ],
        ..State::default()
    }
}

#[test]
fn messages_update_the_state_views_read() {
    let mut demo = load(COUNTER).unwrap();
    assert_eq!(demo.topics(), [Topic::Agents]);
    let now = SystemTime::UNIX_EPOCH;
    assert_eq!(demo.view(&agents(), now).unwrap(), Some(text("0/2")));
    assert!(demo.update(&"add".into(), now).unwrap().is_empty());
    assert!(demo.update(&"keep".into(), now).unwrap().is_empty());
    demo.update(&"add".into(), now).unwrap();
    assert_eq!(demo.view(&agents(), now).unwrap(), Some(text("2/2")));
}

#[test]
fn update_returns_effects() {
    let mut demo = load(COUNTER).unwrap();
    let effects = demo
        .update(&"later".into(), SystemTime::UNIX_EPOCH)
        .unwrap();
    assert_eq!(
        effects,
        [
            Effect::Timer {
                after: Duration::from_secs(5),
                msg: "add".into()
            },
            Effect::Run(herdr_commands::Command::NewWindow),
        ]
    );
}

#[test]
fn the_log_is_bounded_and_drained() {
    let mut demo = load(COUNTER).unwrap();
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(60);
    for _ in 0..250 {
        demo.update(&"other".into(), now).unwrap();
    }
    let log = demo.take_log();
    assert_eq!(log.len(), 200);
    assert_eq!(
        log[0],
        LogLine {
            at: now,
            text: "got other 0".to_owned()
        }
    );
    assert!(demo.take_log().is_empty());
}

#[test]
fn unwatched_topics_are_absent_and_the_app_is_read_only() {
    let main = view_mod("ui.text(tostring(app.usage))");
    let mut demo = load(&main).unwrap();
    assert_eq!(
        demo.view(&agents(), SystemTime::UNIX_EPOCH).unwrap(),
        Some(text("nil"))
    );
    let main = "local ui = require(\"@herdr/ui\")\n\
        return herdr.mod { watch = { \"agents\" },\n\
          view = function(_, app) app.agents.all[1] = nil return ui.spacer() end }";
    let error = load(main)
        .unwrap()
        .view(&agents(), SystemTime::UNIX_EPOCH)
        .unwrap_err();
    assert!(matches!(error, Error::Lua(_)), "{error:?}");
}

#[test]
fn now_is_the_callers_time() {
    let main = view_mod("ui.text(tostring(herdr.now()))");
    let at = SystemTime::UNIX_EPOCH + Duration::from_secs(90);
    assert_eq!(
        load(&main).unwrap().view(&State::default(), at).unwrap(),
        Some(text("90"))
    );
}

#[test]
fn require_loads_the_mods_own_modules_once() {
    let sources = Sources::new(files(&[
        ("mod.toml", MANIFEST),
        (
            "main.luau",
            "local parts = require(\"./parts\")\nassert(require(\"parts\") == parts)\n\
             return herdr.mod { view = function() return parts.label() end }",
        ),
        (
            "parts.luau",
            "local ui = require(\"@herdr/ui\")\nreturn { label = function() return ui.text(\"part\") end }",
        ),
    ]))
    .unwrap();
    let mut demo = Mod::load(&sources, Limits::default(), SystemTime::UNIX_EPOCH).unwrap();
    assert_eq!(
        demo.view(&State::default(), SystemTime::UNIX_EPOCH)
            .unwrap(),
        Some(text("part"))
    );
}

#[test]
fn require_refuses_other_files_and_cycles() {
    for name in ["main", "../other", "/etc/passwd", "@herdr/fs"] {
        let error = load(&format!("require(\"{name}\")")).unwrap_err();
        assert!(matches!(error, Error::Lua(_)), "{name}: {error:?}");
    }
    let sources = Sources::new(files(&[
        ("mod.toml", MANIFEST),
        ("main.luau", "require(\"./a\")"),
        ("a.luau", "return require(\"./b\")"),
        ("b.luau", "return require(\"./a\")"),
    ]))
    .unwrap();
    let error = Mod::load(&sources, Limits::default(), SystemTime::UNIX_EPOCH).unwrap_err();
    assert!(format!("{error}").contains("cycle"), "{error}");
}

#[test]
fn the_spec_is_checked() {
    let error = load("return 1").unwrap_err();
    assert!(matches!(error, Error::NotAMod), "{error:?}");
    for main in [
        "return herdr.mod { }",
        "return herdr.mod { view = 1 }",
        "return herdr.mod { view = function() end, render = function() end }",
        "return herdr.mod { view = function() end, watch = { \"files\" } }",
        "herdr.mod { view = function() end }\nreturn herdr.mod { view = function() end }",
    ] {
        let error = load(main).unwrap_err();
        assert!(matches!(error, Error::Lua(_)), "{main}: {error:?}");
    }
}

#[test]
fn a_mod_without_update_refuses_messages() {
    let mut demo = load(&view_mod("ui.spacer()")).unwrap();
    let error = demo
        .update(&"x".into(), SystemTime::UNIX_EPOCH)
        .unwrap_err();
    assert!(matches!(error, Error::Message { .. }), "{error:?}");
}

#[test]
fn a_view_error_keeps_the_mod_usable() {
    let main = "local ui = require(\"@herdr/ui\")\n\
        return herdr.mod { init = function() return { fail = true } end,\n\
          update = function(state) state.fail = false return state end,\n\
          view = function(state) if state.fail then error(\"boom\") end return ui.spacer() end }";
    let mut demo = load(main).unwrap();
    let now = SystemTime::UNIX_EPOCH;
    let error = demo.view(&State::default(), now).unwrap_err();
    assert!(format!("{error}").contains("main.luau:4: boom"), "{error}");
    demo.update(&Data::Nil, now).unwrap();
    assert_eq!(
        demo.view(&State::default(), now).unwrap(),
        Some(Node::Spacer)
    );
}

#[test]
fn globals_and_libraries_are_sealed() {
    let error = load("herdr.run = nil").unwrap_err();
    assert!(matches!(error, Error::Lua(_)), "{error:?}");
    load(&format!(
        "assert(os == nil and io == nil and print == nil and loadstring == nil)\n{}",
        view_mod("ui.spacer()")
    ))
    .unwrap();
}

#[test]
fn log_lines_are_bounded_without_copying_whole_arguments() {
    // A 1 MiB string passed 1,024 times would be about 1 GiB if copied.
    let main = format!(
        "local s = string.rep(\"x\", 1024 * 1024)\n\
         local args = table.create(1024, s)\n\
         herdr.log(table.unpack(args))\n\
         herdr.log(\"n\", 1, 2.5, true, nil, {{}}, setmetatable({{}}, {{ __tostring = function() return string.rep(\"y\", 1e6) end }}))\n{}",
        view_mod("ui.spacer()")
    );
    let mut demo = load(&main).unwrap();
    let log = demo.take_log();
    assert_eq!(log[0].text.len(), 1024);
    assert_eq!(log[1].text, "n 1 2.5 true nil table table");
}

#[test]
fn a_view_that_returns_nil_draws_nothing() {
    let main = "return herdr.mod({ view = function() return nil end })";
    assert_eq!(
        load(main)
            .unwrap()
            .view(&State::default(), SystemTime::UNIX_EPOCH)
            .unwrap(),
        None
    );
}
