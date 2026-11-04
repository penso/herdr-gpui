//! Turning a GraphQL response into a `PullRequest`, rejecting anything whose
//! repository or branch does not match what was asked for.

use super::{MergeMethod, PullRequest, Result, State, clean, fetch::OUTPUT_LIMIT};
use crate::Error;

pub(super) fn parse_graphql(
    response: serde_json::Value,
    owner: &str,
    repo: &str,
    branch: &str,
    head: Option<&super::fetch::Head>,
) -> Result {
    if response["data"]["repository"]["pullRequests"]["pageInfo"]["hasNextPage"] == true {
        return Err(super::Error::Ambiguous.into());
    }
    let mut nodes = response["data"]["repository"]["pullRequests"]["nodes"]
        .as_array()
        .ok_or(super::Error::Repository)?
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
        &serde_json::to_string(&nodes).map_err(Error::json)?,
        owner,
        repo,
        branch,
        head.map_or(owner, |head| head.owner.as_str()),
    )?;
    if let Some(pr) = &mut result {
        if incomplete {
            pr.checks_summary
                .push_str(" (first 100; more checks exist)");
        }
        let repository = &response["data"]["repository"];
        pr.merge_methods = MergeMethod::ALL
            .into_iter()
            .filter(|method| repository[method.setting()] == true)
            .collect();
    }
    Ok(result)
}

/// A pull request looked up by number for a local fork branch. Only a fork's
/// pull request can be the one that branch was created for; it is then held to
/// the same head identity checks as one found through a configured upstream.
pub(super) fn parse_numbered(
    mut response: serde_json::Value,
    owner: &str,
    repo: &str,
    number: u64,
) -> Result {
    let pr = response["data"]["repository"]["pullRequest"].take();
    if pr.is_null() {
        return Ok(None);
    }
    if pr["isCrossRepository"] != true && pr["number"].as_u64() == Some(number) {
        return Ok(None);
    }
    response["data"]["repository"]["pullRequest"] = pr;
    parse_by_number(response, owner, repo, number)
}

/// Any pull request of `owner/repo` by number, held to the same identity
/// checks against its own head repository and branch.
pub(super) fn parse_by_number(
    mut response: serde_json::Value,
    owner: &str,
    repo: &str,
    number: u64,
) -> Result {
    let pr = response["data"]["repository"]["pullRequest"].take();
    if pr.is_null() {
        return Ok(None);
    }
    if pr["number"].as_u64() != Some(number) {
        return Err(super::Error::Identity.into());
    }
    let (Some(head_owner), Some(head_repo), Some(branch)) = (
        pr["headRepositoryOwner"]["login"].as_str(),
        pr["headRepository"]["name"].as_str(),
        pr["headRefName"].as_str(),
    ) else {
        // A deleted fork leaves no head to verify against.
        return Ok(None);
    };
    let head = super::fetch::Head {
        owner: head_owner.to_owned(),
        repo: head_repo.to_owned(),
        branch: branch.to_owned(),
    };
    response["data"]["repository"]["pullRequests"] = serde_json::json!({"nodes": [pr]});
    parse_graphql(response, owner, repo, &head.branch, Some(&head))
}

#[cfg(any(test, feature = "test-support"))]
pub(super) fn parse(text: &str, owner: &str, repo: &str, branch: &str) -> Result {
    parse_with_owner(text, owner, repo, branch, owner)
}

fn parse_with_owner(text: &str, owner: &str, repo: &str, branch: &str, head_owner: &str) -> Result {
    if text.len() > OUTPUT_LIMIT {
        return Err(super::Error::Size.into());
    }
    let mut values: Vec<PullRequest> = serde_json::from_str(text).map_err(Error::json)?;
    // A lone pull request from another head owner is held to the identity
    // check below; among several, other owners' ones are not this branch's.
    if values.len() > 1 {
        values.retain(|pr| {
            pr.head_repository_owner
                .login
                .eq_ignore_ascii_case(head_owner)
        });
        if values.is_empty() {
            return Err(super::Error::Ambiguous.into());
        }
    }
    let Some(mut pr) = current(values, |pr| pr.state == State::Open)? else {
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
        return Err(super::Error::Identity.into());
    }
    pr.url = expected;
    pr.title = clean(&pr.title);
    pr.head_ref_name = clean(&pr.head_ref_name);
    pr.base_ref_name = clean(&pr.base_ref_name);
    pr.updated_at = clean(&pr.updated_at);
    // Both identifiers are sent back to GitHub, so anything unexpected is
    // dropped rather than cleaned into a different value.
    if !node_id(&pr.id) {
        pr.id.clear();
    }
    if !object_id(&pr.head_ref_oid) {
        pr.head_ref_oid.clear();
    }
    for check in pr.status_check_rollup.iter_mut().flatten() {
        check.name = clean(&check.name).chars().take(128).collect();
    }
    pr.checks_summary = pr.checks();
    Ok(Some(pr))
}

/// The one pull request a branch stands for, among those already narrowed to
/// its head. A branch name reused after its first pull request merged or
/// closed lists both, so the single open one wins, or, with none open, the
/// most recently updated, which GitHub lists first. Two open ones stay
/// ambiguous.
pub(super) fn current<T>(
    mut values: Vec<T>,
    open: impl Fn(&T) -> bool,
) -> crate::Result<Option<T>> {
    let open: Vec<usize> = (0..values.len())
        .filter(|&index| open(&values[index]))
        .collect();
    match open.as_slice() {
        [index] => Ok(Some(values.swap_remove(*index))),
        [] if values.is_empty() => Ok(None),
        [] => Ok(Some(values.swap_remove(0))),
        _ => Err(super::Error::Ambiguous.into()),
    }
}

/// GitHub node IDs are short base64url-like tokens.
pub(super) fn node_id(id: &str) -> bool {
    (1..=128).contains(&id.len())
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'='))
}

/// A SHA-1 or SHA-256 commit ID in lowercase hex.
pub(super) fn object_id(oid: &str) -> bool {
    matches!(oid.len(), 40 | 64)
        && oid
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

// Nonblocking sockets avoid reader threads that can hang on inherited pipe handles.
// Kill/wait only the exact child we created, and never on the UI thread.
