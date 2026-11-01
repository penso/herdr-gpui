//! Whom Azure DevOps takes a token for in an organization, and the projects
//! it shows there: what the account menu checks before it saves a pasted
//! token, and shows for each organization.

use super::{Error, HOST, TIMEOUT, agent, body, token::Token};
use serde::Deserialize;

/// The answers here are small; anything larger is refused.
const LIMIT: u64 = 2 * 1024 * 1024;
/// The most project names one check reads.
pub const MAX_PROJECTS: usize = 500;

/// The account a token signs in as in an organization.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Account {
    /// The name Azure DevOps shows for the account.
    pub name: String,
    /// The sign-in address, when Azure DevOps gives one.
    pub email: Option<String>,
    /// The organization's projects the token can see, sorted, or `None` when
    /// it may not list them, which reading pull requests does not need.
    pub projects: Option<Vec<String>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConnectionData {
    authenticated_user: Option<User>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct User {
    provider_display_name: Option<String>,
    custom_display_name: Option<String>,
    #[serde(default)]
    properties: Properties,
}

#[derive(Default, Deserialize)]
struct Properties {
    #[serde(rename = "Account")]
    account: Option<Property>,
}

#[derive(Deserialize)]
struct Property {
    #[serde(rename = "$value")]
    value: Option<String>,
}

#[derive(Deserialize)]
struct Projects {
    value: Vec<Project>,
}

#[derive(Deserialize)]
struct Project {
    name: String,
}

/// Whom `organization` takes `token` for, and its projects. Blocks on the
/// network: call it from a worker.
pub fn check(organization: &str, token: &Token) -> Result<Account, Error> {
    check_at(&format!("https://{HOST}"), organization, token)
}

/// [`check`] against Azure DevOps at `origin`, which tests point at a local
/// server.
pub fn check_at(origin: &str, organization: &str, token: &Token) -> Result<Account, Error> {
    let url = |path: &[&str]| {
        let mut url =
            url::Url::parse(origin).map_err(|_| Error::Organization(organization.to_owned()))?;
        url.path_segments_mut()
            .map_err(|()| Error::Organization(organization.to_owned()))?
            .push(organization)
            .extend(path);
        Ok::<_, Error>(url)
    };
    let connection: ConnectionData = get(
        origin,
        organization,
        token,
        url(&["_apis", "connectionData"])?,
    )?;
    let user = connection.authenticated_user.ok_or(Error::Authentication)?;
    let name = user
        .custom_display_name
        .filter(|name| !name.trim().is_empty())
        .or(user.provider_display_name)
        .filter(|name| !name.trim().is_empty())
        .ok_or(Error::Authentication)?;
    let email = user
        .properties
        .account
        .and_then(|property| property.value)
        .filter(|value| value.contains('@'));
    let mut projects_url = url(&["_apis", "projects"])?;
    projects_url
        .query_pairs_mut()
        .append_pair("$top", &MAX_PROJECTS.to_string())
        .append_pair("api-version", super::API_VERSION);
    let projects = match get::<Projects>(origin, organization, token, projects_url) {
        Ok(projects) => {
            let mut names: Vec<String> = projects
                .value
                .into_iter()
                .map(|project| project.name)
                .take(MAX_PROJECTS)
                .collect();
            names.sort_by_key(|name| name.to_lowercase());
            Some(names)
        }
        // A token scoped to Code and Work Items may not list projects.
        Err(Error::Authentication | Error::Forbidden { .. }) => None,
        Err(error) => return Err(error),
    };
    Ok(Account {
        name,
        email,
        projects,
    })
}

fn get<T: serde::de::DeserializeOwned>(
    origin: &str,
    organization: &str,
    token: &Token,
    url: url::Url,
) -> Result<T, Error> {
    let mut reply = agent(TIMEOUT)
        .get(url.as_str())
        .header("User-Agent", "Herdr-GPUI")
        .header("Accept", "application/json")
        .header("Authorization", token.authorization()?)
        .call()
        .map_err(|error| Error::Network {
            origin: origin.to_owned(),
            source: Box::new(error),
        })?;
    match reply.status().as_u16() {
        200 => {}
        404 => return Err(Error::NoOrganization(organization.to_owned())),
        status => return Err(super::status_error(status, organization)),
    }
    serde_json::from_slice(&body(&mut reply, LIMIT)?).map_err(Error::Json)
}
