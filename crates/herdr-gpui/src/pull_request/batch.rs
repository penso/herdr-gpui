//! Many checkouts' lookups in one GraphQL request. Each checkout is an aliased
//! `repository` field, so a refresh of twenty worktrees is one round trip,
//! and lookups GitHub prices below a point share one instead of costing one
//! each. Every alias is still parsed and verified like a single lookup.

use super::Error;
use super::{
    Input, Origin, Origins, Result,
    fetch::{Ask, FIELDS, Head, NUMBER_QUERY, TIMEOUT, Target, target},
    parse::current,
    parse_graphql, parse_numbered,
};
use crate::github::Tokens;
use secrecy::SecretString;
use serde_json::{Value, json};
use std::{
    fmt::Write as _,
    sync::Arc,
    time::{Duration, Instant},
};

/// The most checkouts one request asks for. A branch lookup reads at most a
/// few hundred nodes, so twenty stay well inside GitHub's node limit.
pub(super) const BATCH_LIMIT: usize = 20;

/// Pull requests read per branch with an upstream. GitHub prices a lookup by
/// how many it may return, and few branch names have more across forks; one
/// that does is looked up again (see [`crowded`]).
pub(super) const HEAD_LIMIT: u64 = 5;

/// How long a batch spends identifying its checkouts before it asks GitHub,
/// so one slow SSH device cannot hold every other checkout's answer back.
const PREPARE: Duration = TIMEOUT;

const SETTINGS: &str = "mergeCommitAllowed squashMergeAllowed rebaseMergeAllowed";

/// Only the heads of a crowded branch name's pull requests: cheap to list.
const HEADS: &str = r#"query($owner: String!, $repo: String!, $branch: String!) {
  repository(owner: $owner, name: $repo) {
    pullRequests(first: 100, headRefName: $branch, orderBy: {field: UPDATED_AT, direction: DESC}) {
      pageInfo { hasNextPage }
      nodes { number state headRepositoryOwner { login } headRepository { name } }
    }
  }
}"#;

/// What a batch found: each checkout's own answer; the checkouts a failure of
/// a whole request (an exhausted quota, the network) left unanswered, which
/// share its error; those it never asked about, which stay due; and the
/// accounts GitHub paused, each with only its own checkouts.
#[derive(Default)]
pub(super) struct Batch {
    pub found: Vec<(Input, Result)>,
    pub failed: Vec<(Vec<Input>, crate::Error)>,
    pub deferred: Vec<Input>,
    pub paused: Vec<Paused>,
    /// Checkouts read by an account that was already paused, never asked,
    /// with that account's login.
    pub held: Vec<(Input, String)>,
}

/// An account GitHub asked to wait, for how long, and the checkouts its
/// requests carried, whose failures say why.
pub(super) struct Paused {
    pub login: String,
    pub cooldown: Duration,
    pub inputs: Vec<Input>,
}

/// Asks each account about the checkouts it reads. A pause is one account's:
/// it holds that account's remaining checkouts, never another's, and an
/// account in `paused` is not asked at all.
pub(super) fn fetch_batch(
    inputs: Vec<Input>,
    origin: &Origin,
    origins: &mut Origins,
    tokens: &Tokens,
    paused: &[String],
    cancelled: impl Fn() -> bool,
) -> Batch {
    let mut batch = Batch::default();
    let targets = prepare(
        inputs,
        Instant::now() + PREPARE,
        Instant::now,
        |input, deadline| target(input, origin, origins, deadline, &cancelled),
        &mut batch,
    );
    for (login, token, targets) in by_token(targets, tokens) {
        if paused.contains(&login) {
            batch
                .held
                .extend(targets.into_iter().map(|(input, _)| (input, login.clone())));
            continue;
        }
        let inputs: Vec<Input> = targets.iter().map(|(input, _)| input.clone()).collect();
        let mut cooldown = None;
        ask(
            targets,
            |query, variables, aliases, cooldown| {
                crate::github::graphql_aliases(
                    &token, query, variables, aliases, TIMEOUT, &cancelled, cooldown,
                )
            },
            |response, target, pause| answer_for(response, target, &token, &cancelled, pause),
            (&mut batch, &mut cooldown),
        );
        if let Some(cooldown) = cooldown {
            batch.paused.push(Paused {
                login,
                cooldown,
                inputs,
            });
        }
    }
    batch
}

/// One account's login and token, and the targets it reads.
pub(super) type Group = (String, Arc<SecretString>, Vec<(Input, Target)>);

/// The targets each account reads, in the order their first one came: a
/// request carries one token, so each account's repositories are asked in
/// requests of their own.
pub(super) fn by_token(targets: Vec<(Input, Target)>, tokens: &Tokens) -> Vec<Group> {
    let mut groups: Vec<Group> = Vec::new();
    for (input, target) in targets {
        let token = tokens.for_owner(&target.owner);
        match groups
            .iter_mut()
            .find(|(_, known, _)| Arc::ptr_eq(known, token))
        {
            Some((_, _, group)) => group.push((input, target)),
            None => groups.push((
                tokens.login_for(&target.owner).to_owned(),
                token.clone(),
                vec![(input, target)],
            )),
        }
    }
    groups
}

/// Identifies each checkout until `deadline`. A checkout Git cannot identify
/// fails alone; one the deadline leaves unprepared is deferred, not failed.
pub(super) fn prepare(
    inputs: Vec<Input>,
    deadline: Instant,
    now: impl Fn() -> Instant,
    mut target: impl FnMut(&Input, Instant) -> crate::Result<Target>,
    batch: &mut Batch,
) -> Vec<(Input, Target)> {
    let mut targets = Vec::with_capacity(inputs.len());
    for input in inputs {
        if now() >= deadline {
            batch.deferred.push(input);
            continue;
        }
        match target(&input, deadline) {
            Ok(target) => targets.push((input, target)),
            Err(error) => batch.found.push((input, Err(error))),
        }
    }
    targets
}

/// Keeps the longest pause any request of a batch was given: a later
/// request that succeeds must not lift one an earlier request set.
fn hold(cooldown: &mut Option<Duration>, pause: Option<Duration>) {
    *cooldown = (*cooldown).max(pause);
}

/// Asks about every target. Each response is bounded as one lookup's was;
/// many busy checkouts together may not fit, so a group too large is halved
/// and asked again. Once GitHub asks for a pause nothing more is requested:
/// the remaining groups, and answers that would need another request, wait
/// for a later batch, which the pause holds back.
///
/// `answer` is given a cooldown to set when it may ask GitHub again, and
/// `None` once the batch is paused, when it returns `None` if it would need
/// to.
pub(super) fn ask(
    targets: Vec<(Input, Target)>,
    mut request: impl FnMut(
        &str,
        Value,
        &[(String, String)],
        &mut Option<Duration>,
    ) -> crate::Result<Vec<crate::Result<Value>>>,
    mut answer: impl FnMut(Value, &Target, Option<&mut Option<Duration>>) -> Option<Result>,
    (batch, cooldown): (&mut Batch, &mut Option<Duration>),
) {
    let mut groups = vec![targets];
    while let Some(mut group) = groups.pop() {
        if group.is_empty() {
            continue;
        }
        if cooldown.is_some() {
            batch
                .deferred
                .extend(group.into_iter().map(|(input, _)| input));
            continue;
        }
        let (query, variables, aliases) = query(&group);
        let mut pause = None;
        let answers = request(&query, variables, &aliases, &mut pause);
        hold(cooldown, pause);
        match answers {
            Ok(answers) => {
                for ((input, target), response) in group.into_iter().zip(answers) {
                    let response = match response {
                        Ok(response) => response,
                        Err(error) => {
                            batch.found.push((input, Err(error)));
                            continue;
                        }
                    };
                    let mut pause = None;
                    let network = cooldown.is_none().then_some(&mut pause);
                    let result = answer(response, &target, network);
                    hold(cooldown, pause);
                    match result {
                        Some(result) => batch.found.push((input, result)),
                        None => batch.deferred.push(input),
                    }
                }
            }
            Err(crate::Error::GitHub(crate::github::Error::Size)) if group.len() > 1 => {
                let second = group.split_off(group.len() / 2);
                groups.push(second);
                groups.push(group);
            }
            Err(error) => {
                batch
                    .failed
                    .push((group.into_iter().map(|(input, _)| input).collect(), error));
                batch
                    .deferred
                    .extend(groups.drain(..).flatten().map(|(input, _)| input));
            }
        }
    }
}

/// One alias's pull request, looked up again when its branch name is
/// crowded, which `network` (`None` while paused) must allow.
fn answer_for(
    response: Value,
    target: &Target,
    token: &SecretString,
    cancelled: &impl Fn() -> bool,
    network: Option<&mut Option<Duration>>,
) -> Option<Result> {
    if let Ask::Branch {
        branch,
        head: Some(head),
    } = &target.ask
        && response["data"]["repository"]["pullRequests"]["pageInfo"]["hasNextPage"] == true
    {
        return network.map(|cooldown| crowded(target, branch, head, token, cancelled, cooldown));
    }
    Some(parse(response, target))
}

/// A branch name more than [`HEAD_LIMIT`] pull requests share across forks:
/// list only their heads, then read the one the upstream names in full. Two
/// cheap requests, and only for such a branch.
fn crowded(
    target: &Target,
    branch: &str,
    head: &Head,
    token: &SecretString,
    cancelled: &impl Fn() -> bool,
    cooldown: &mut Option<Duration>,
) -> Result {
    let (owner, repo) = (target.owner.as_str(), target.repo.as_str());
    let mut pause = None;
    let heads = crate::github::graphql(
        "pull_request",
        token,
        HEADS,
        json!({"owner": owner, "repo": repo, "branch": branch}),
        TIMEOUT,
        cancelled,
        &mut pause,
    );
    hold(cooldown, pause);
    let heads = heads?;
    let number = matching_number(&heads, head)?;
    let Some(number) = number else {
        return Ok(None);
    };
    let mut pause = None;
    let response = crate::github::graphql(
        "pull_request",
        token,
        NUMBER_QUERY,
        json!({"owner": owner, "repo": repo, "number": number}),
        TIMEOUT,
        cancelled,
        &mut pause,
    );
    hold(cooldown, pause);
    let mut response = response?;
    let pr = response["data"]["repository"]["pullRequest"].take();
    if pr.is_null() {
        return Ok(None);
    }
    response["data"]["repository"]["pullRequests"] = json!({"nodes": [pr]});
    parse_graphql(response, owner, repo, branch, Some(head))
}

/// The listed pull request from `head` its branch stands for, if any.
pub(super) fn matching_number(heads: &Value, head: &Head) -> crate::Result<Option<u64>> {
    let listing = &heads["data"]["repository"]["pullRequests"];
    if listing["pageInfo"]["hasNextPage"] == true {
        return Err(Error::Ambiguous.into());
    }
    let heads: Vec<&Value> = listing["nodes"]
        .as_array()
        .ok_or(Error::Repository)?
        .iter()
        .filter(|pr| {
            pr["headRepositoryOwner"]["login"]
                .as_str()
                .is_some_and(|owner| owner.eq_ignore_ascii_case(&head.owner))
                && pr["headRepository"]["name"]
                    .as_str()
                    .is_some_and(|repo| repo.eq_ignore_ascii_case(&head.repo))
                && pr["number"].is_u64()
        })
        .collect();
    Ok(current(heads, |pr| pr["state"] == "OPEN")?.and_then(|pr| pr["number"].as_u64()))
}

pub(super) fn parse(response: Value, target: &Target) -> Result {
    match &target.ask {
        Ask::Branch { branch, head } => {
            parse_graphql(response, &target.owner, &target.repo, branch, head.as_ref())
        }
        Ask::Number(number) => parse_numbered(response, &target.owner, &target.repo, *number),
    }
}

/// The query, its variables, and each alias with the `owner/repo` it reads.
/// Names and branches travel only as variables, never in the query text.
pub(super) fn query(targets: &[(Input, Target)]) -> (String, Value, Vec<(String, String)>) {
    let mut declarations = Vec::with_capacity(targets.len());
    let mut fields = String::new();
    let mut variables = serde_json::Map::new();
    let mut aliases = Vec::with_capacity(targets.len());
    for (index, (_, target)) in targets.iter().enumerate() {
        variables.insert(format!("o{index}"), target.owner.clone().into());
        variables.insert(format!("r{index}"), target.repo.clone().into());
        let lookup = match &target.ask {
            Ask::Branch { branch, head } => {
                declarations.push(format!(
                    "$o{index}: String!, $r{index}: String!, $b{index}: String!, $l{index}: Int!"
                ));
                variables.insert(format!("b{index}"), branch.clone().into());
                // An upstream head names its owner, so other forks' pull
                // requests from a branch of the same name are filtered out;
                // without one, two are enough to tell an ambiguous branch.
                let limit = if head.is_some() { HEAD_LIMIT } else { 2 };
                variables.insert(format!("l{index}"), limit.into());
                format!(
                    "pullRequests(first: $l{index}, headRefName: $b{index}, orderBy: {{field: UPDATED_AT, direction: DESC}}) {{ pageInfo {{ hasNextPage }} nodes {{ ...PullRequestFields }} }}"
                )
            }
            Ask::Number(number) => {
                declarations.push(format!(
                    "$o{index}: String!, $r{index}: String!, $n{index}: Int!"
                ));
                variables.insert(format!("n{index}"), (*number).into());
                format!("pullRequest(number: $n{index}) {{ ...PullRequestFields }}")
            }
        };
        let _ = writeln!(
            fields,
            "  p{index}: repository(owner: $o{index}, name: $r{index}) {{ {SETTINGS} {lookup} }}"
        );
        aliases.push((
            format!("p{index}"),
            format!("{}/{}", target.owner, target.repo),
        ));
    }
    let query = format!("query({}) {{\n{fields}}}{FIELDS}", declarations.join(", "));
    (query, Value::Object(variables), aliases)
}
