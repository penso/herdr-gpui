use super::*;
use crate::cloud::batch::Error;

#[test]
fn every_cloud_batch_variant_has_a_message() {
    let output = || "apt-get: not found".to_owned();
    assert_localized(&[
        Error::NotOffered("Nimbus"),
        Error::Setup {
            code: Some(2),
            output: String::new(),
        },
        Error::Setup {
            code: Some(127),
            output: output(),
        },
        Error::Setup {
            code: None,
            output: String::new(),
        },
        Error::Setup {
            code: None,
            output: output(),
        },
    ]);
}

/// `herdr_remote::cloud::exec::Error` has no mapping of its own yet.
#[test]
fn exec_errors_show_their_display() {
    assert_verbatim(&[Error::Exec(crate::cloud::exec::Error::Empty)]);
}
