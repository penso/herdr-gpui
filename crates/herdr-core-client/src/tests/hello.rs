use crate::{
    Method,
    hello::{Hello, Mismatch},
};

fn hello(generation: u32, version: &str, methods: &[&str]) -> Hello {
    let mut reply = super::hello_reply(version, methods)["result"].clone();
    reply["generation"] = generation.into();
    serde_json::from_value(reply).unwrap()
}

#[test]
fn a_current_core_with_every_method_is_accepted() {
    assert_eq!(hello(1, "0.1.13", &super::every_method()).check(), Ok(()));
    assert_eq!(
        hello(1, "hc 0.2.0-beta", &super::every_method()).check(),
        Ok(())
    );
}

#[test]
fn an_old_or_newer_generation_is_refused() {
    let methods = super::every_method();
    assert_eq!(
        hello(2, "0.1.13", &methods).check(),
        Err(Mismatch::Newer(2))
    );
    assert_eq!(
        hello(0, "0.1.13", &methods).check(),
        Err(Mismatch::TooOld {
            have: "0.1.13".into()
        })
    );
    assert_eq!(
        hello(1, "0.1.12", &methods).check(),
        Err(Mismatch::TooOld {
            have: "0.1.12".into()
        })
    );
}

#[test]
fn missing_methods_are_named() {
    let methods: Vec<&str> = super::every_method()
        .into_iter()
        .filter(|name| *name != "doc.set")
        .collect();
    assert_eq!(
        hello(1, "0.1.13", &methods).check(),
        Err(Mismatch::Missing(vec![Method::DocSet]))
    );
    // The handshake itself is never listed as missing.
    let methods: Vec<&str> = super::every_method()
        .into_iter()
        .filter(|name| *name != "hello")
        .collect();
    assert_eq!(hello(1, "0.1.13", &methods).check(), Ok(()));
}
