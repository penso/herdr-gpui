//! Sandbox data-plane access. Owning a subscription does not let anyone use
//! a sandbox group's data plane: that takes the Container Apps SandboxGroup
//! Data Owner role on the group. When this app creates a group, it grants the
//! role to the identity it signed in as, the way the Azure portal does for
//! the person who creates one.

use super::super::{Error, Result, Settings, http::Client};
use base64::Engine as _;
use secrecy::ExposeSecret;
use serde::Deserialize;

/// Container Apps SandboxGroup Data Owner, a built-in role.
const DATA_OWNER_ROLE: &str = "c24cf47c-5077-412d-a19c-45202126392c";
const API_VERSION: &str = "2022-04-01";

#[derive(Debug, Deserialize, PartialEq, Eq)]
pub(super) struct Principal {
    #[serde(rename = "oid")]
    pub(super) id: String,
    /// "app" for a service principal, "user" for a person.
    #[serde(default, rename = "idtyp")]
    kind: Option<String>,
}

impl Principal {
    fn principal_type(&self) -> &'static str {
        match self.kind.as_deref() {
            Some("app") => "ServicePrincipal",
            _ => "User",
        }
    }
}

/// Who an access token was issued to, from its unverified claims. The token
/// came straight from Entra ID or the Azure CLI, so it names this app's own
/// identity; nothing here decides what that identity may do.
pub(super) fn principal(token: &str) -> Result<Principal> {
    let claims = token
        .split('.')
        .nth(1)
        .ok_or(Error::Field("Azure access token"))?;
    let claims = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(claims.trim_end_matches('='))
        .map_err(|_| Error::Field("Azure access token"))?;
    serde_json::from_slice(&claims).map_err(Error::Json)
}

/// Grant the signed-in identity the data role on the sandbox group. Granting
/// it again is harmless.
pub(super) fn grant(client: &Client, settings: &Settings) -> Result<()> {
    let principal = principal(client.token().expose_secret())?;
    let scope = format!(
        "/subscriptions/{}/resourceGroups/{}/providers/Microsoft.App/sandboxGroups/{}",
        settings.subscription_id, settings.resource_group, settings.sandbox_group
    );
    // One assignment per group and identity, so a retry names the same one.
    let name = uuid::Uuid::new_v5(
        &uuid::Uuid::NAMESPACE_URL,
        format!("{scope}#{}", principal.id).as_bytes(),
    );
    let path = format!(
        "{scope}/providers/Microsoft.Authorization/roleAssignments/{name}?api-version={API_VERSION}"
    );
    let body = serde_json::json!({
        "properties": {
            "roleDefinitionId": format!(
                "/subscriptions/{}/providers/Microsoft.Authorization/roleDefinitions/{DATA_OWNER_ROLE}",
                settings.subscription_id
            ),
            "principalId": principal.id,
            "principalType": principal.principal_type(),
        }
    });
    match client.put::<_, serde_json::Value>(&path, &body) {
        // 409: the identity already holds the role under another name.
        Ok(_) | Err(Error::Status(409, _)) => Ok(()),
        Err(error) => Err(error),
    }
}
