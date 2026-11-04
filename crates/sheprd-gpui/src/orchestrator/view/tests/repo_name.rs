use crate::orchestrator::view::repo::last_part;

#[test]
fn a_repository_is_named_by_its_last_folder_on_every_system() {
    assert_eq!(last_part("penso/app"), "app");
    assert_eq!(last_part("/home/me/code/app/"), "app");
    assert_eq!(
        last_part(r"C:\Users\RUNNER~1\AppData\Local\Temp\.tmpwsS4CE\app"),
        "app"
    );
    assert_eq!(last_part(r"C:\code\app\"), "app");
    assert_eq!(last_part("app"), "app");
}
