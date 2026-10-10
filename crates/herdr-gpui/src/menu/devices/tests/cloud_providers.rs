use super::*;
use crate::cloud::CloudProvider;

#[gpui::test]
fn provider_suggestions_follow_configuration_changes(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            for &provider in CloudProvider::ALL {
                let (url, variable) = match provider {
                    #[cfg(feature = "coder")]
                    CloudProvider::Coder => (&mut view.config.coder.url, "HERDR_CODER_URL"),
                    #[cfg(feature = "daytona")]
                    CloudProvider::Daytona => {
                        (&mut view.config.daytona.api_url, "HERDR_DAYTONA_API_URL")
                    }
                };
                *url = None;
                let available = crate::cloud::unavailable().is_none();
                let from_environment = std::env::var_os(variable).is_some();
                assert_eq!(
                    view.cloud_providers().contains(&provider),
                    available && from_environment,
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
                assert_eq!(view.cloud_providers().contains(&provider), available);

                match provider {
                    #[cfg(feature = "coder")]
                    CloudProvider::Coder => view.config.coder.url = None,
                    #[cfg(feature = "daytona")]
                    CloudProvider::Daytona => view.config.daytona.api_url = None,
                }
                assert_eq!(
                    view.cloud_providers().contains(&provider),
                    available && from_environment,
                    "removing configuration must hide the suggestion again",
                );
                assert_eq!(view.cloud_provider_rows(), view.cloud_providers().len());
            }
        });
    });
}
