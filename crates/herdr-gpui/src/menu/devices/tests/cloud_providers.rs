use super::*;
use crate::cloud::CloudProvider;

#[gpui::test]
fn provider_suggestions_follow_configuration_changes(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            for &provider in CloudProvider::ALL {
                let url = match provider {
                    #[cfg(feature = "coder")]
                    CloudProvider::Coder => &mut view.config.coder.url,
                    #[cfg(feature = "daytona")]
                    CloudProvider::Daytona => &mut view.config.daytona.api_url,
                };
                *url = None;
                let available = crate::cloud::unavailable().is_none();
                assert!(
                    !view.cloud_providers_with_env(|_| None).contains(&provider),
                    "{provider:?} must not be suggested without configuration",
                );

                match provider {
                    #[cfg(feature = "coder")]
                    CloudProvider::Coder => {
                        view.config.coder.url = Some("https://coder.example.com".into());
                    }
                    #[cfg(feature = "daytona")]
                    CloudProvider::Daytona => {
                        view.config.daytona.api_url = Some("https://app.daytona.io/api".into());
                    }
                }
                assert_eq!(
                    view.cloud_providers_with_env(|_| None).contains(&provider),
                    available,
                );

                match provider {
                    #[cfg(feature = "coder")]
                    CloudProvider::Coder => view.config.coder.url = None,
                    #[cfg(feature = "daytona")]
                    CloudProvider::Daytona => view.config.daytona.api_url = None,
                }
                assert!(
                    !view.cloud_providers_with_env(|_| None).contains(&provider),
                    "removing configuration must hide the suggestion again",
                );
                assert_eq!(view.cloud_provider_rows(), view.cloud_providers().len());
            }
        });
    });
}

#[gpui::test]
fn provider_suggestions_follow_environment_only_configuration(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.config.coder.url = None;
            view.config.daytona.api_url = None;
            for &provider in CloudProvider::ALL {
                let (variable, url) = match provider {
                    #[cfg(feature = "coder")]
                    CloudProvider::Coder => ("HERDR_CODER_URL", "https://coder.example.com"),
                    #[cfg(feature = "daytona")]
                    CloudProvider::Daytona => {
                        ("HERDR_DAYTONA_API_URL", "https://app.daytona.io/api")
                    }
                };
                let expected = if crate::cloud::unavailable().is_none() {
                    vec![provider]
                } else {
                    Vec::new()
                };
                assert_eq!(
                    view.cloud_providers_with_env(|name| (name == variable).then(|| url.into())),
                    expected,
                    "{variable} alone must suggest only {provider:?}",
                );
                assert!(
                    view.cloud_providers_with_env(|_| None).is_empty(),
                    "removing {variable} must hide its suggestion again",
                );
            }
            assert_eq!(
                view.cloud_providers_with_env(|name| match name {
                    "HERDR_CODER_URL" => Some("https://coder.example.com".into()),
                    "HERDR_DAYTONA_API_URL" => Some("https://app.daytona.io/api".into()),
                    _ => None,
                }),
                if crate::cloud::unavailable().is_none() {
                    CloudProvider::ALL.to_vec()
                } else {
                    Vec::new()
                },
                "environment-only providers must retain picker order",
            );
        });
    });
}
