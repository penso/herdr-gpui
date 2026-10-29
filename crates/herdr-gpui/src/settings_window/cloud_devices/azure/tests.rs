use super::{Account, AzureConfig, AzureFields, SignIn, sign_in};

mod destroy;
mod messages;

fn fields(subscription: &str, client: &str) -> AzureFields {
    AzureFields {
        subscription_id: subscription.into(),
        client_id: client.into(),
        ..AzureFields::default()
    }
}

const SUBSCRIPTION: &str = "00000000-0000-0000-0000-000000000000";
const CLIENT: &str = "11111111-1111-1111-1111-111111111111";

#[test]
fn saving_blank_ids_with_az_installed_chooses_the_cli() {
    let az = Account::Unconfigured { az_available: true };
    let none = AzureConfig::default();
    assert_eq!(sign_in(&az, &fields("", ""), &none), SignIn::Cli);
    let no_az = Account::Unconfigured {
        az_available: false,
    };
    assert_eq!(sign_in(&no_az, &fields("", ""), &none), SignIn::Keep);
    assert_eq!(sign_in(&az, &fields(SUBSCRIPTION, ""), &none), SignIn::Keep);
}

#[test]
fn a_client_id_replaces_a_saved_cli_choice() {
    let az = Account::Unconfigured { az_available: true };
    let cli = AzureConfig {
        use_azure_cli: Some(true),
        ..AzureConfig::default()
    };
    assert_eq!(
        sign_in(&az, &fields(SUBSCRIPTION, CLIENT), &cli),
        SignIn::ServicePrincipal
    );
    assert_eq!(
        sign_in(&az, &fields(SUBSCRIPTION, CLIENT), &AzureConfig::default()),
        SignIn::Keep
    );
}
mod progress;
