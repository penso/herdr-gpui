use super::*;
use crate::cloud::CloudProvider;

/// The setting that makes `provider` suggested, or none for an account-free one.
fn setting(view: &mut HerdrWindow, provider: CloudProvider) -> Option<&mut Option<String>> {
    match provider {
        #[cfg(feature = "azure")]
        CloudProvider::Azure => Some(&mut view.config.azure.subscription_id),
        #[cfg(feature = "boat")]
        CloudProvider::Boat => Some(&mut view.config.boat.api_url),
        #[cfg(feature = "coder")]
        CloudProvider::Coder => Some(&mut view.config.coder.url),
        #[cfg(feature = "daytona")]
        CloudProvider::Daytona => Some(&mut view.config.daytona.api_url),
        #[cfg(feature = "railway")]
        CloudProvider::Railway => {
            let _ = view;
            None
        }
    }
}

/// An example account URL for `provider`, as `(environment variable, value)`.
fn account(provider: CloudProvider) -> Option<(&'static str, &'static str)> {
    match provider {
        #[cfg(feature = "azure")]
        CloudProvider::Azure => Some((
            "HERDR_AZURE_SUBSCRIPTION_ID",
            "00000000-0000-0000-0000-000000000000",
        )),
        #[cfg(feature = "boat")]
        CloudProvider::Boat => Some(("HERDR_BOAT_API_URL", "https://boat.dev/api/v1")),
        #[cfg(feature = "coder")]
        CloudProvider::Coder => Some(("HERDR_CODER_URL", "https://coder.example.com")),
        #[cfg(feature = "daytona")]
        CloudProvider::Daytona => Some(("HERDR_DAYTONA_API_URL", "https://app.daytona.io/api")),
        #[cfg(feature = "railway")]
        CloudProvider::Railway => None,
    }
}

fn available() -> bool {
    crate::cloud::unavailable().is_none()
}

/// What the picker offers when only `configured` is set up, in picker order.
/// An account-free provider is never configured: its rail entry says Set up.
fn expected(configured: &[CloudProvider]) -> Vec<CloudProvider> {
    if !available() {
        return Vec::new();
    }
    CloudProvider::ALL
        .iter()
        .copied()
        .filter(|provider| configured.contains(provider))
        .collect()
}

#[gpui::test]
fn provider_suggestions_follow_configuration_changes(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            for &provider in CloudProvider::ALL {
                let Some((_, url)) = account(provider) else {
                    // No configuration to change.
                    assert!(
                        !view.cloud_providers_with_env(|_| None).contains(&provider),
                        "an account-free provider is never shown as configured",
                    );
                    assert!(setting(view, provider).is_none());
                    continue;
                };
                *setting(view, provider).unwrap() = None;
                assert!(
                    !view.cloud_providers_with_env(|_| None).contains(&provider),
                    "{provider:?} must not be suggested without configuration",
                );
                *setting(view, provider).unwrap() = Some(url.into());
                assert_eq!(
                    view.cloud_providers_with_env(|_| None).contains(&provider),
                    available(),
                );
                *setting(view, provider).unwrap() = None;
                assert!(
                    !view.cloud_providers_with_env(|_| None).contains(&provider),
                    "removing configuration must hide the suggestion again",
                );
            }
        });
    });
}

#[gpui::test]
fn provider_suggestions_follow_environment_only_configuration(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            for &provider in CloudProvider::ALL {
                if let Some(url) = setting(view, provider) {
                    *url = None;
                }
            }
            let mut configured = Vec::new();
            let mut with_accounts = Vec::new();
            for &provider in CloudProvider::ALL {
                // Nothing to configure, in the environment or the file.
                let Some((variable, url)) = account(provider) else {
                    continue;
                };
                configured.push((variable, url));
                with_accounts.push(provider);
                assert_eq!(
                    view.cloud_providers_with_env(|name| (name == variable).then(|| url.into())),
                    expected(&[provider]),
                    "{variable} alone must suggest only {provider:?}",
                );
                assert_eq!(
                    view.cloud_providers_with_env(|_| None),
                    expected(&[]),
                    "removing {variable} must hide its suggestion again",
                );
            }
            assert_eq!(
                view.cloud_providers_with_env(|name| configured
                    .iter()
                    .find(|(variable, _)| *variable == name)
                    .map(|(_, url)| (*url).into())),
                expected(&with_accounts),
                "environment-only providers must retain picker order",
            );
        });
    });
}
