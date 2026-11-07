use super::*;

#[test]
fn a_machine_tab_is_listed_by_its_kind_and_name() {
    let scope = Scope::local(Path::new("/tmp/herdr.sock"));
    let mut store = Store::default();
    let name = crate::machine::Name::try_from("work".to_owned()).unwrap();
    store.open(scope, "w_1", Some(Location::Machine { name }), None);
    let listed: Vec<TabInfo> = store.tabs().map(info).collect();
    assert_eq!(listed[0].kind, TabKind::Machine);
    assert_eq!(listed[0].location.as_deref(), Some("work"));
    assert!(!TabKind::Machine.is_page());
}
