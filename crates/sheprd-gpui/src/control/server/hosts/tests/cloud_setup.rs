//! `cloud setup`: runs the configured setup script on a saved cloud device
//! again, refusing a name that is no cloud device or a build with no script.
use super::*;

fn add_cloud(view: &mut SheprdWindow) -> String {
    let provider = crate::cloud::CloudProvider::ALL[0];
    let id = format!("{}:w1", provider.key());
    view.endpoints.push(crate::endpoint::Endpoint::new(
        id.clone(),
        "Build box".into(),
        sheprd_client::ConnectTarget::Cloud {
            provider,
            account: "https://cloud.example.com".into(),
            id: "w1".into(),
            machine: "herdr-tidy-lark-aa".into(),
            session: "default".into(),
        },
        true,
    ));
    id
}

#[gpui::test]
fn cloud_setup_finds_the_device_by_id_machine_or_label(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            add_cloud(view);
            let local = cloud_setup(view, crate::endpoint::LOCAL, cx);
            assert_eq!(error_code(&local), Some(ErrorCode::NotFound));
            let unset = cloud_setup(view, "build box", cx);
            assert_eq!(error_code(&unset), Some(ErrorCode::InvalidRequest));
            view.config.cloud.setup = Some("echo hi".into());
            for name in ["w1", "Build Box"] {
                let response = cloud_setup(view, name, cx);
                assert_eq!(error_code(&response), None, "{name}: {response:?}");
            }
        })
    });
}
