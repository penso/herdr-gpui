use super::*;

fn round_trip(expression: &str) -> std::result::Result<Data, (String, Problem)> {
    let lua = mlua::Lua::new();
    let value = lua.load(expression).eval::<mlua::Value>().unwrap();
    let data = Data::read(&value)?;
    let back = data.to_lua(&lua).unwrap();
    assert_eq!(Data::read(&back).unwrap(), data);
    Ok(data)
}

#[test]
fn plain_values_round_trip() {
    assert_eq!(round_trip("nil").unwrap(), Data::Nil);
    assert_eq!(round_trip("3").unwrap(), Data::Integer(3));
    assert_eq!(
        round_trip("{ 1, 'a' }").unwrap(),
        Data::List(vec![Data::Integer(1), "a".into()])
    );
    let Data::Map(map) = round_trip("{ id = 'x', on = true, n = 1.5 }").unwrap() else {
        panic!("expected a map");
    };
    assert_eq!(map["n"], Data::Number(1.5));
}

#[test]
fn mixed_sparse_and_opaque_values_are_refused() {
    for expression in [
        "{ 1, a = 2 }",
        "{ [3] = 1 }",
        "{ [true] = 1 }",
        "function() end",
    ] {
        assert!(round_trip(expression).is_err(), "{expression}");
    }
}

#[test]
fn values_are_bounded() {
    let (_, problem) =
        round_trip("(function() local t = {} for i = 1, 300 do t[i] = i end return t end)()")
            .unwrap_err();
    assert_eq!(problem, Problem::Entries { limit: 256 });
    let (_, problem) =
        round_trip("(function() local t = 1 for i = 1, 10 do t = { t } end return t end)()")
            .unwrap_err();
    assert_eq!(problem, Problem::Depth { limit: 8 });
}
