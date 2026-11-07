use super::*;

#[test]
fn cancelled_boat_scripts_never_prepare_credentials_or_run_commands() {
    let error = with_command::<()>(
        CloudProvider::Boat,
        "https://boat.example/api/v1",
        "bx_23456789",
        &AtomicBool::new(true),
        |_| panic!("cancelled scripts must not run"),
    )
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Interrupted);
    assert!(matches!(
        error
            .get_ref()
            .and_then(|source| source.downcast_ref::<crate::cloud::Error>()),
        Some(crate::cloud::Error::Cancelled)
    ));
}
