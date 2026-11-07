use super::*;

#[test]
fn unsupported_creation_fails_before_reading_keys_or_reporting_creation() {
    let result = add_device(
        AddRequest {
            settings: crate::boat::tests::settings(),
            target: Target::Create("must-not-be-created".into()),
            install: true,
        },
        &|| false,
        &|_| panic!("must not start setup"),
    );
    assert!(matches!(
        result,
        Err(crate::Error::Cloud(cloud::Error::Unavailable(_)))
    ));
}
