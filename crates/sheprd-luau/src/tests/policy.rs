use super::*;

/// Gone under every policy: files, processes, environments, code loading.
const ALWAYS_GONE: &[&str] = &[
    "os",
    "io",
    "debug",
    "coroutine",
    "require",
    "load",
    "loadstring",
    "dofile",
    "print",
    "collectgarbage",
    "gcinfo",
    "getfenv",
    "setfenv",
    "newproxy",
];

const SAFE: &[&str] = &[
    "string.upper",
    "table.concat",
    "math.floor",
    "utf8.len",
    "bit32.band",
    "buffer.create",
];

#[test]
fn deterministic_removes_catching_and_randomness() {
    let (lua, _) = sealed(Policy::Deterministic, Limits::default());
    let gone = [
        ALWAYS_GONE,
        &["pcall", "xpcall", "math.random", "math.randomseed"],
    ]
    .concat();
    lua.load(absent(&gone) + &present(SAFE)).exec().unwrap();
}

#[test]
fn interactive_keeps_catching_and_randomness() {
    let (lua, _) = sealed(Policy::Interactive, Limits::default());
    let kept = [SAFE, &["pcall", "xpcall", "math.random"]].concat();
    lua.load(absent(ALWAYS_GONE) + &present(&kept))
        .exec()
        .unwrap();
}

#[test]
fn sealed_libraries_cannot_be_changed() {
    let (lua, _) = sealed(Policy::Interactive, Limits::default());
    let error = lua.load("string.upper = nil").exec().unwrap_err();
    assert!(matches!(error, mlua::Error::RuntimeError(_)), "{error:?}");
}

#[test]
fn a_caught_budget_error_still_stops_the_script() {
    let limits = Limits {
        interrupts: 10_000,
        ..Limits::default()
    };
    let (lua, _) = sealed(Policy::Interactive, limits);
    let error = lua
        .load("pcall(function() while true do end end)\nwhile true do end")
        .exec()
        .unwrap_err();
    assert_eq!(
        error.downcast_ref::<OutOfInstructions>(),
        Some(&OutOfInstructions)
    );
}

#[test]
fn bytecode_is_never_loaded() {
    let bytecode = mlua::chunk::Compiler::new().compile("return 42").unwrap();
    assert!(
        bytecode[0] < b'\n',
        "Luau bytecode starts with its version byte"
    );
    // The default loader would run it; ours must treat it as text.
    let source = as_text(&bytecode);
    let (lua, _) = sealed(Policy::Interactive, Limits::default());
    let error = text(&lua, &source, "evil").eval::<i64>().unwrap_err();
    assert!(
        matches!(error, mlua::Error::SyntaxError { .. }),
        "{error:?}"
    );
}

/// Bytecode as the UTF-8 text a mod file would have to be.
fn as_text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}
