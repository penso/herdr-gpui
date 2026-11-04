use super::*;
use crate::cloud::Value;

#[test]
fn invalid_auto_stop_values_return_field_errors_instead_of_panicking() {
    for value in [
        "30分",
        "30ｍ",
        "💤",
        "分m",
        "３０m",
        "m",
        "0h",
        "18446744073709551615h",
    ] {
        let config = ContainerConfig {
            auto_stop: Some(value.into()),
            ..Default::default()
        };
        assert!(
            matches!(
                Settings::resolve(&config),
                Err(Error::Field(Value::Key("[container] auto_stop")))
            ),
            "{value}"
        );
    }
    assert_eq!(super::super::settings::auto_stop(""), None);
}
