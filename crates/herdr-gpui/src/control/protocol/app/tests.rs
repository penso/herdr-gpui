#![allow(clippy::unwrap_used)]
use super::*;
use crate::control::protocol::{Call, Caller, Request};

#[test]
fn open_carries_its_folder_and_refuses_extra_fields() {
    let request = Request::App(Call {
        caller: Caller::default(),
        op: AppOp::Open {
            path: r"C:\Users\me\project".into(),
        },
    });
    let text = serde_json::to_string(&request).unwrap();
    assert!(text.contains(r#""op":"open""#), "{text}");
    assert_eq!(serde_json::from_str::<Request>(&text).unwrap(), request);
    for invalid in [
        r#"{"method":"app","caller":{},"op":{"op":"open"}}"#,
        r#"{"method":"app","caller":{},"op":{"op":"open","path":"/a","cwd":"/b"}}"#,
    ] {
        assert!(
            serde_json::from_str::<Request>(invalid).is_err(),
            "{invalid}"
        );
    }
}
