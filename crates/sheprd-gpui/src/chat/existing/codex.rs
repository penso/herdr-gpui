//! A conflicting launch identity cannot establish ownership of a transcript.

use uuid::Uuid;

/// `resume UUID` must agree with the daemon. A picker, named session, `--last`,
/// or unknown option cannot supply that check, so it keeps the terminal.
/// Bare/new sessions and `fork` still use the existing daemon/file checks:
/// a fork's argument names its parent, not the conversation it creates.
/// This is only a conflict guard: argv cannot attest to an in-process switch.
pub(super) fn session_matches(args: &[String], advertised: &str) -> bool {
    let Ok(advertised) = Uuid::parse_str(advertised) else {
        return false;
    };
    // Leave other launch modes under the existing daemon/file verification.
    // A token inside a prompt or option value is disambiguated below.
    if !args.iter().any(|arg| arg == "resume") {
        return true;
    }
    let mut args = args.iter().map(String::as_str);
    let mut resume = false;
    let mut session = None;
    let mut literal = false;
    let mut prompt = false;
    while let Some(arg) = args.next() {
        if !literal && arg == "--" {
            literal = true;
            continue;
        }
        if !literal && arg.starts_with('-') {
            if !skip_option(arg, &mut args) {
                return false;
            }
            continue;
        }
        if !resume {
            if !literal && arg == "resume" {
                resume = true;
                continue;
            }
            return true;
        }
        if session.is_none() {
            let Ok(id) = Uuid::parse_str(arg) else {
                return false;
            };
            if id != advertised {
                return false;
            }
            session = Some(id);
        } else if prompt {
            return false;
        } else {
            prompt = true;
        }
    }
    !resume || session.is_some()
}

/// Only known fixed-arity options can be skipped without mistaking a value
/// (or an image list) for the resume selector. New CLI options fail closed.
fn skip_option<'a>(arg: &'a str, args: &mut impl Iterator<Item = &'a str>) -> bool {
    let (option, inline) = arg
        .split_once('=')
        .map_or((arg, None), |(name, value)| (name, Some(value)));
    match option {
        "-c"
        | "--config"
        | "-m"
        | "--model"
        | "-p"
        | "--profile"
        | "-s"
        | "--sandbox"
        | "-C"
        | "--cd"
        | "-a"
        | "--ask-for-approval"
        | "--enable"
        | "--disable"
        | "--add-dir"
        | "--remote"
        | "--remote-auth-token-env"
        | "--local-provider" => match inline {
            Some(value) => !value.is_empty(),
            None => args
                .next()
                .is_some_and(|value| !value.is_empty() && !value.starts_with('-')),
        },
        "--all"
        | "--include-non-interactive"
        | "--strict-config"
        | "--oss"
        | "--approve-for-me"
        | "--dangerously-bypass-approvals-and-sandbox"
        | "--dangerously-bypass-hook-trust"
        | "--full-auto"
        | "--worktree"
        | "--search"
        | "--no-alt-screen"
        | "--no-daemon" => inline.is_none(),
        _ => false,
    }
}
