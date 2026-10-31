use super::*;

mod budgets;
mod policy;

fn sealed(policy: Policy, limits: Limits) -> (mlua::Lua, Meter) {
    let meter = Meter::default();
    let lua = open(limits, policy, &meter).unwrap();
    seal(&lua).unwrap();
    meter.reset();
    (lua, meter)
}

fn absent(names: &[&str]) -> String {
    names
        .iter()
        .map(|name| format!("assert({name} == nil, \"{name} is still reachable\")\n"))
        .collect()
}

fn present(names: &[&str]) -> String {
    names
        .iter()
        .map(|name| format!("assert({name} ~= nil, \"{name} is missing\")\n"))
        .collect()
}
