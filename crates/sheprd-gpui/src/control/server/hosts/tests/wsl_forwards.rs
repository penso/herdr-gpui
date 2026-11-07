use super::*;

#[gpui::test]
fn a_distribution_needs_no_forward_and_says_why(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.endpoints.push(crate::endpoint::Endpoint::new(
                "wsl:Ubuntu".into(),
                "Ubuntu".into(),
                sheprd_client::ConnectTarget::Wsl {
                    distro: "Ubuntu".into(),
                    session: "default".into(),
                },
                true,
            ));
            view.selected_endpoint = 1;
            let refused = super::super::ports::forward(view, &caller(None), 3000, cx);
            assert_eq!(error_code(&refused), Some(ErrorCode::InvalidRequest));
            let Response::Error { message, .. } = refused else {
                panic!("expected a refusal");
            };
            assert_eq!(
                message,
                "WSL already forwards Ubuntu's ports to this machine; open them at localhost"
            );
            assert_eq!(view.port_forwards.for_host("Ubuntu").count(), 0);
        })
    });
}
