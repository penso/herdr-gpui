use super::*;
use crate::cloud::ProviderError;

#[test]
fn already_cancelled_jobs_exit_before_preparing_ssh_or_credentials() {
    let request = AddRequest {
        settings: crate::boat::tests::settings(),
        target: Target::Create("herdr-box".into()),
        install: true,
    };
    let result = add_device(request, &|| true, &|_| {
        panic!("cancelled jobs must not report creation")
    });
    assert!(matches!(
        result,
        Err(crate::Error::Provider(ProviderError::Boat(
            crate::boat::Error::Cancelled
        )))
    ));
}
