//! Turning a GraphQL response into a `PullRequest`, rejecting anything whose
//! repository or branch does not match what was asked for.

use super::{PullRequest, Result, State, clean, fetch::OUTPUT_LIMIT};
use crate::Error;

pub(super) fn parse_graphql(
    response: serde_json::Value,
    owner: &str,
    repo: &str,
    branch: &str,
    head: Option<&super::fetch::Head>,
) -> Result {
    if response["data"]["repository"]["pullRequests"]["pageInfo"]["hasNextPage"] == true {
        return Err(Error::PrAmbiguous);
    }
    let mut nodes = response["data"]["repository"]["pullRequests"]["nodes"]
        .as_array()
        .ok_or(Error::PrRepository)?
        .clone();
    if let Some(head) = head {
        nodes.retain(|pr| {
            pr["headRefName"].as_str() == Some(head.branch.as_str())
                && pr["headRepositoryOwner"]["login"]
                    .as_str()
                    .is_some_and(|owner| owner.eq_ignore_ascii_case(&head.owner))
                && pr["headRepository"]["name"]
                    .as_str()
                    .is_some_and(|repo| repo.eq_ignore_ascii_case(&head.repo))
        });
    }
    let mut incomplete = false;
    for pr in &mut nodes {
        let contexts = &pr["commits"]["nodes"][0]["commit"]["statusCheckRollup"]["contexts"];
        incomplete |= contexts["pageInfo"]["hasNextPage"] == true;
        let checks = contexts["nodes"].clone();
        pr["statusCheckRollup"] = checks;
    }
    let mut result = parse_with_owner(
        &serde_json::to_string(&nodes).map_err(Error::github_json)?,
        owner,
        repo,
        branch,
        head.map_or(owner, |head| head.owner.as_str()),
    )?;
    if incomplete && let Some(pr) = &mut result {
        pr.checks_summary
            .push_str(" (first 100; more checks exist)");
    }
    Ok(result)
}

#[cfg(any(test, all(feature = "integration-test", target_os = "macos")))]
pub(super) fn parse(text: &str, owner: &str, repo: &str, branch: &str) -> Result {
    parse_with_owner(text, owner, repo, branch, owner)
}

fn parse_with_owner(text: &str, owner: &str, repo: &str, branch: &str, head_owner: &str) -> Result {
    if text.len() > OUTPUT_LIMIT {
        return Err(Error::PrSize);
    }
    let mut values: Vec<PullRequest> = serde_json::from_str(text).map_err(Error::github_json)?;
    if values.is_empty() {
        return Ok(None);
    }
    if values.len() != 1 {
        return Err(Error::PrAmbiguous);
    }
    let Some(mut pr) = values.pop() else {
        return Ok(None);
    };
    let expected = format!("https://github.com/{owner}/{repo}/pull/{}", pr.number);
    if pr.number == 0
        || pr.state == State::Unknown
        || !pr.url.eq_ignore_ascii_case(&expected)
        || pr.head_ref_name != branch
        || !pr
            .head_repository_owner
            .login
            .eq_ignore_ascii_case(head_owner)
    {
        return Err(Error::PrIdentity);
    }
    pr.url = expected;
    pr.title = clean(&pr.title);
    pr.head_ref_name = clean(&pr.head_ref_name);
    pr.base_ref_name = clean(&pr.base_ref_name);
    pr.updated_at = clean(&pr.updated_at);
    pr.checks_summary = pr.checks();
    Ok(Some(pr))
}

// Nonblocking sockets avoid reader threads that can hang on inherited pipe handles.
// Kill/wait only the exact child we created, and never on the UI thread.
