//! Reading one alert's or report's page from GitHub, on a background thread:
//! an alert's advisory over GraphQL, a report's draft advisory and its
//! comments over REST. Remote text is untrusted: bodies become the release
//! notes' bounded Markdown lines, short strings are cleaned and bounded,
//! lists are capped, and links are kept only when they are `https`.

use super::super::{Error, model::ghsa};
use crate::{
    github::Tokens,
    release_notes::{self, Line},
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(20);
/// Weaknesses, identifiers, references, credits, and affected packages kept
/// of each page: an advisory lists a handful.
const MAX_LIST: usize = 20;
/// Comments read of a report's conversation: GitHub's first page.
pub(crate) const MAX_COMMENTS: usize = 50;
/// The longest link kept.
const MAX_URL: usize = 1024;

// i18n: data
const ALERT_QUERY: &str = r#"query($owner: String!, $repo: String!, $number: Int!) {
  repository(owner: $owner, name: $repo) {
    vulnerabilityAlert(number: $number) {
      vulnerableRequirements dependencyRelationship dependencyScope
      dependabotUpdate { pullRequest { number } }
      securityAdvisory {
        description publishedAt
        cvssSeverities { cvssV3 { score vectorString } cvssV4 { score vectorString } }
        cwes(first: 20) { nodes { cweId name } }
        identifiers { type value }
        references { url }
      }
    }
  }
}"#;

/// What an alert's page shows beyond its listing row.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AlertPage {
    pub(crate) description: Vec<Line>,
    pub(crate) published_at: Option<DateTime<Utc>>,
    pub(crate) cvss: Option<Cvss>,
    pub(crate) weaknesses: Vec<Weakness>,
    /// CVE and GHSA identifiers, as `CVE-2024-12224`.
    pub(crate) identifiers: Vec<String>,
    pub(crate) references: Vec<String>,
    /// What the manifest asks for, as `= 0.102.6` in a lockfile.
    pub(crate) requirement: Option<String>,
    pub(crate) relationship: Relationship,
    pub(crate) scope: Scope,
    /// The pull request Dependabot opened to fix it, if any.
    pub(crate) update: Option<u64>,
}

/// What a private report's page shows beyond its listing row.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ReportPage {
    pub(crate) description: Vec<Line>,
    pub(crate) affected: Vec<Affected>,
    pub(crate) cvss: Option<Cvss>,
    pub(crate) weaknesses: Vec<Weakness>,
    pub(crate) credits: Vec<Credit>,
    pub(crate) comments: Comments,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Cvss {
    /// `4.0` or `3.1`, whichever GitHub scored; 4.0 first.
    pub(crate) version: &'static str,
    pub(crate) score: f64,
    pub(crate) vector: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Weakness {
    /// `CWE-295`.
    pub(crate) id: String,
    pub(crate) name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Affected {
    pub(crate) package: String,
    pub(crate) ecosystem: String,
    pub(crate) vulnerable: String,
    pub(crate) patched: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Credit {
    pub(crate) login: String,
    /// `finder`, `reporter`, `remediation_developer`, as GitHub names it.
    pub(crate) kind: String,
}

/// A report's conversation, as far as the REST API shows it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Comments {
    /// GitHub counted none.
    None,
    /// Oldest first; `more` when GitHub holds more than were read.
    Read { comments: Vec<Comment>, more: bool },
    /// The comments endpoint refused: it is a preview, for public
    /// repositories only.
    Unavailable,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Comment {
    pub(crate) author: Option<String>,
    pub(crate) body: Vec<Line>,
    pub(crate) created_at: Option<DateTime<Utc>>,
}

/// Whether the vulnerable package is one the manifest names.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub(crate) enum Relationship {
    Direct,
    Transitive,
    #[default]
    #[serde(other)]
    Unknown,
}

/// Whether the package ships or only builds and tests.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub(crate) enum Scope {
    Runtime,
    Development,
    #[default]
    #[serde(other)]
    Unknown,
}

/// The page of alert `number` of `name`, a checked `owner/repo`.
pub(super) fn alert(
    name: &str,
    tokens: &Tokens,
    number: u64,
    cancelled: impl Fn() -> bool,
) -> Result<AlertPage, Error> {
    let (owner, repo) = name.split_once('/').ok_or(Error::Output)?;
    let mut cooldown = None;
    let response = crate::github::graphql(
        "security",
        tokens.for_owner(owner),
        ALERT_QUERY,
        serde_json::json!({"owner": owner, "repo": repo, "number": number}),
        TIMEOUT,
        cancelled,
        &mut cooldown,
    )
    .map_err(|error| Error::Github(Box::new(error.into())))?;
    parse_alert(&response)
}

/// The page of report `advisory` of `name`, a checked `owner/repo`. The
/// conversation is read only when GitHub counts comments, and its refusal
/// leaves the rest of the page.
pub(super) fn report(
    name: &str,
    tokens: &Tokens,
    advisory: &str,
    cancelled: impl Fn() -> bool,
) -> Result<ReportPage, Error> {
    let (owner, _) = name.split_once('/').ok_or(Error::Output)?;
    if !ghsa(advisory) {
        return Err(Error::Output);
    }
    let token = tokens.for_owner(owner);
    let path = format!("/repos/{name}/security-advisories/{advisory}");
    let node: AdvisoryNode = crate::github::rest("security", token, &path, TIMEOUT, &cancelled)
        .map_err(|error| Error::Github(Box::new(error.into())))?;
    let count = node.comments;
    let mut page = node.into_page();
    if count != Some(0) {
        page.comments = match crate::github::rest::<Vec<CommentNode>>(
            "security",
            token,
            &format!("{path}/comments?per_page={}", MAX_COMMENTS + 1),
            TIMEOUT,
            &cancelled,
        ) {
            Ok(nodes) => comments(nodes),
            Err(error) => {
                tracing::debug!(category = "security", %error, "Cannot read a report's comments");
                Comments::Unavailable
            }
        };
    }
    Ok(page)
}

pub(crate) fn parse_alert(response: &Value) -> Result<AlertPage, Error> {
    let node = response
        .get("data")
        .and_then(|data| data.get("repository"))
        .and_then(|repository| repository.get("vulnerabilityAlert"))
        .filter(|alert| !alert.is_null())
        .ok_or(Error::Output)?;
    let node = AlertNode::deserialize(node)?;
    let advisory = node.security_advisory;
    Ok(AlertPage {
        description: body(advisory.as_ref().and_then(|a| a.description.as_deref())),
        published_at: advisory.as_ref().and_then(|a| a.published_at),
        cvss: advisory
            .as_ref()
            .and_then(|a| a.cvss_severities.as_ref())
            .and_then(|cvss| {
                score("4.0", cvss.cvss_v4.as_ref()).or_else(|| score("3.1", cvss.cvss_v3.as_ref()))
            }),
        weaknesses: advisory
            .as_ref()
            .and_then(|a| a.cwes.as_ref())
            .map(|cwes| {
                weaknesses(
                    cwes.nodes
                        .iter()
                        .flatten()
                        .map(|cwe| (&cwe.cwe_id, &cwe.name)),
                )
            })
            .unwrap_or_default(),
        identifiers: advisory
            .as_ref()
            .map(|a| {
                a.identifiers
                    .iter()
                    .map(|id| clean(&id.value))
                    .filter(|id| !id.is_empty())
                    .take(MAX_LIST)
                    .collect()
            })
            .unwrap_or_default(),
        references: advisory
            .as_ref()
            .map(|a| links(a.references.iter().map(|reference| reference.url.as_str())))
            .unwrap_or_default(),
        requirement: node
            .vulnerable_requirements
            .as_deref()
            .map(clean)
            .filter(|text| !text.is_empty()),
        relationship: node.dependency_relationship.unwrap_or_default(),
        scope: node.dependency_scope.unwrap_or_default(),
        update: node
            .dependabot_update
            .and_then(|update| update.pull_request)
            .map(|pull| pull.number),
    })
}

pub(crate) fn comments(nodes: Vec<CommentNode>) -> Comments {
    let more = nodes.len() > MAX_COMMENTS;
    let mut comments: Vec<Comment> = nodes
        .into_iter()
        .take(MAX_COMMENTS)
        .map(|node| Comment {
            author: node
                .user
                .map(|user| clean(&user.login))
                .filter(|login| !login.is_empty()),
            body: body(node.body.as_deref()),
            created_at: node.created_at,
        })
        .collect();
    comments.sort_by_key(|comment| comment.created_at);
    Comments::Read { comments, more }
}

/// Untrusted Markdown as bounded lines, without the HTML GitHub bodies hold.
/// A link that is not `https` stays as text but is not clickable.
pub(crate) fn body(text: Option<&str>) -> Vec<Line> {
    let mut lines = text
        .map(|text| release_notes::parse(&crate::orchestrator::without_html(text)))
        .unwrap_or_default();
    for line in &mut lines {
        line.links.retain(|link| link.url.starts_with("https://"));
    }
    lines
}

fn clean(text: &str) -> String {
    crate::pull_request::clean(text.trim())
}

fn score(version: &'static str, cvss: Option<&CvssNode>) -> Option<Cvss> {
    let cvss = cvss?;
    let score = cvss
        .score
        .filter(|score| (0.0..=10.0).contains(score) && *score > 0.0)?;
    Some(Cvss {
        version,
        score,
        vector: cvss.vector_string.as_deref().map(clean).unwrap_or_default(),
    })
}

fn weaknesses<'a>(cwes: impl Iterator<Item = (&'a String, &'a String)>) -> Vec<Weakness> {
    cwes.map(|(id, name)| Weakness {
        id: clean(id),
        name: clean(name),
    })
    .filter(|weakness| !weakness.id.is_empty())
    .take(MAX_LIST)
    .collect()
}

/// The `https` links of a list, cleaned; anything else is dropped.
fn links<'a>(urls: impl Iterator<Item = &'a str>) -> Vec<String> {
    urls.map(str::trim)
        .filter(|url| {
            url.len() <= MAX_URL
                && url.starts_with("https://")
                && !url.chars().any(|c| c.is_control() || c.is_whitespace())
        })
        .map(clean)
        .take(MAX_LIST)
        .collect()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AlertNode {
    vulnerable_requirements: Option<String>,
    dependency_relationship: Option<Relationship>,
    dependency_scope: Option<Scope>,
    dependabot_update: Option<UpdateNode>,
    security_advisory: Option<SecurityAdvisoryNode>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateNode {
    pull_request: Option<PullNode>,
}

#[derive(Deserialize)]
struct PullNode {
    number: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SecurityAdvisoryNode {
    description: Option<String>,
    published_at: Option<DateTime<Utc>>,
    cvss_severities: Option<CvssSeveritiesNode>,
    cwes: Option<CweConnection>,
    #[serde(default)]
    identifiers: Vec<IdentifierNode>,
    #[serde(default)]
    references: Vec<ReferenceNode>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CvssSeveritiesNode {
    #[serde(alias = "cvss_v3")]
    cvss_v3: Option<CvssNode>,
    #[serde(alias = "cvss_v4")]
    cvss_v4: Option<CvssNode>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CvssNode {
    score: Option<f64>,
    #[serde(alias = "vector_string")]
    vector_string: Option<String>,
}

#[derive(Deserialize)]
struct CweConnection {
    nodes: Vec<Option<CweNode>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CweNode {
    #[serde(alias = "cwe_id")]
    cwe_id: String,
    name: String,
}

#[derive(Deserialize)]
struct IdentifierNode {
    value: String,
}

#[derive(Deserialize)]
struct ReferenceNode {
    url: String,
}

/// A repository advisory as REST answers it, the draft a report became.
#[derive(Deserialize)]
pub(crate) struct AdvisoryNode {
    description: Option<String>,
    #[serde(default)]
    vulnerabilities: Option<Vec<VulnerabilityNode>>,
    cvss_severities: Option<CvssSeveritiesNode>,
    #[serde(default)]
    cwes: Option<Vec<CweNode>>,
    #[serde(default)]
    credits: Option<Vec<CreditNode>>,
    /// Non-confidential comments GitHub counts, when it says.
    comments: Option<u64>,
}

#[derive(Deserialize)]
struct VulnerabilityNode {
    package: Option<PackageNode>,
    vulnerable_version_range: Option<String>,
    patched_versions: Option<String>,
}

#[derive(Deserialize)]
struct PackageNode {
    ecosystem: Option<String>,
    name: Option<String>,
}

#[derive(Deserialize)]
struct CreditNode {
    login: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct CommentNode {
    #[serde(alias = "author")]
    user: Option<LoginNode>,
    body: Option<String>,
    created_at: Option<DateTime<Utc>>,
}

#[derive(Deserialize)]
struct LoginNode {
    login: String,
}

impl AdvisoryNode {
    pub(crate) fn into_page(self) -> ReportPage {
        let text = |value: Option<String>| value.as_deref().map(clean).unwrap_or_default();
        ReportPage {
            description: body(self.description.as_deref()),
            affected: self
                .vulnerabilities
                .unwrap_or_default()
                .into_iter()
                .take(MAX_LIST)
                .map(|node| {
                    let (ecosystem, package) = node
                        .package
                        .map(|package| (package.ecosystem, package.name))
                        .unwrap_or_default();
                    Affected {
                        package: text(package),
                        ecosystem: text(ecosystem),
                        vulnerable: text(node.vulnerable_version_range),
                        patched: text(node.patched_versions),
                    }
                })
                .collect(),
            cvss: self.cvss_severities.as_ref().and_then(|cvss| {
                score("4.0", cvss.cvss_v4.as_ref()).or_else(|| score("3.1", cvss.cvss_v3.as_ref()))
            }),
            weaknesses: weaknesses(
                self.cwes
                    .iter()
                    .flatten()
                    .map(|cwe| (&cwe.cwe_id, &cwe.name)),
            ),
            credits: self
                .credits
                .unwrap_or_default()
                .into_iter()
                .filter_map(|credit| {
                    Some(Credit {
                        login: clean(&credit.login?),
                        kind: text(credit.kind).replace('_', " "),
                    })
                })
                .filter(|credit| !credit.login.is_empty())
                .take(MAX_LIST)
                .collect(),
            comments: Comments::None,
        }
    }
}
