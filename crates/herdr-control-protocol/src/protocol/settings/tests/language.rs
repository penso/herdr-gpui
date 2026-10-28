use super::*;

#[test]
fn the_language_request_and_reply_have_a_stable_form() {
    let text = serde_json::to_string(&request(SettingsOp::Language)).unwrap();
    assert_eq!(
        text,
        r#"{"method":"settings","caller":{"pane_id":null,"workspace_id":null,"daemon_socket":null},"op":{"op":"language"}}"#
    );
    assert_eq!(
        serde_json::from_str::<Request>(&text).unwrap(),
        request(SettingsOp::Language)
    );
    let reply = Response::Settings {
        reply: Reply::Language(LanguageInfo {
            configured: "system".into(),
            pseudo: Some("en-XB".into()),
            locale: "en-XB".into(),
            direction: "rtl".into(),
        }),
    };
    let text = serde_json::to_string(&reply).unwrap();
    assert_eq!(
        text,
        r#"{"status":"settings","reply":{"kind":"language","configured":"system","pseudo":"en-XB","locale":"en-XB","direction":"rtl"}}"#
    );
    assert_eq!(serde_json::from_str::<Response>(&text).unwrap(), reply);
    // No pseudo-locale is left off the wire.
    let plain: Response = serde_json::from_str(
        r#"{"status":"settings","reply":{"kind":"language","configured":"he","locale":"en-US","direction":"ltr"}}"#,
    )
    .unwrap();
    assert!(matches!(
        plain,
        Response::Settings {
            reply: Reply::Language(LanguageInfo { pseudo: None, .. })
        }
    ));
}
