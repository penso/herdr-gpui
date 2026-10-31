use super::*;

#[test]
fn every_job_gets_its_own_temporary_names() {
    let first = unique("snapshot");
    let second = unique("snapshot");
    assert_ne!(first, second);
    assert!(first.starts_with("snapshot-"));
}

#[test]
fn each_source_build_unpacks_into_its_own_folder_and_removes_it() {
    let script = build_script("dev-0123456789ab");
    assert!(script.contains(r#"src="$cache/src-$$""#));
    // Removed on every exit, a failed build's included.
    assert!(script.contains(r#"trap 'rm -rf "$src"' EXIT"#));
}
