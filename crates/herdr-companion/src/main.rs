use anyhow::{Context, bail};
use herdr_companion::{
    Command, Companion, Config, Limits, Notifier, ServeOptions, hooks_settings, pairing_url,
    parse_args, phone_urls, render_qr, route_probe, usage,
};
use std::{
    env,
    io::{IsTerminal, stderr},
    net::TcpListener,
    process::ExitCode,
    sync::Arc,
};

/// A guessable token would let anyone who reaches the port approve commands.
const MIN_TOKEN_LEN: usize = 24;
const MAX_CONNECTIONS: usize = 128;

fn main() -> ExitCode {
    let command = match parse_args(env::args_os().skip(1)) {
        Ok(command) => command,
        Err(error) => {
            eprintln!("herdr-companion: {error}\n\n{}", usage());
            return ExitCode::from(2);
        }
    };
    match run(command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("herdr-companion: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(command: Command) -> anyhow::Result<()> {
    let options = match command {
        Command::Help => {
            print!("{}", usage());
            return Ok(());
        }
        Command::Hooks {
            url,
            decision_timeout,
        } => {
            println!("{:#}", hooks_settings(&url, decision_timeout));
            return Ok(());
        }
        Command::Qr { url } => {
            // Asked for explicitly, so it prints even when piped.
            let link = pairing_url(&url, &token()?);
            println!("{}\n{link}", render_qr(&link)?);
            return Ok(());
        }
        Command::Serve(options) => options,
    };
    let token = token()?;
    let listener = TcpListener::bind(options.listen)
        .with_context(|| format!("could not listen on {}", options.listen))?;
    eprintln!("herdr-companion: listening on {}", options.listen);
    // A QR code carries the token, so it never goes to a log file.
    if !options.no_qr && stderr().is_terminal() {
        print_pairing(&options, &token)?;
    } else if !options.listen.ip().is_loopback() {
        eprintln!(
            "herdr-companion: warning: listening beyond loopback over plain HTTP; prefer a \
             Tailscale address or `tailscale serve`"
        );
    }
    let herdr_socket = match options.herdr_socket {
        Some(path) => Some(path),
        None => herdr_companion::default_socket().ok(),
    };
    let notifier = options
        .ntfy
        .map(|topic| Notifier::spawn(topic, options.public_url, options.ntfy_details));
    let companion = Companion::new(Config {
        token: token.into(),
        decision_timeout: options.decision_timeout,
        herdr_socket,
        notifier,
        limits: Limits::default(),
        max_connections: MAX_CONNECTIONS,
    });
    herdr_companion::serve(listener, Arc::new(companion))?;
    Ok(())
}

fn token() -> anyhow::Result<String> {
    let token = env::var("HERDR_COMPANION_TOKEN").context("HERDR_COMPANION_TOKEN is not set")?;
    if token.len() < MIN_TOKEN_LEN {
        bail!("HERDR_COMPANION_TOKEN must be at least {MIN_TOKEN_LEN} characters");
    }
    Ok(token)
}

/// Prints the QR code a phone scans to open the web app signed in, for the
/// best address a phone can reach, and lists the others.
fn print_pairing(options: &ServeOptions, token: &str) -> anyhow::Result<()> {
    let urls = phone_urls(options.listen, options.public_url.as_deref(), route_probe);
    let Some(best) = urls.first() else {
        eprintln!(
            "herdr-companion: no address a phone can reach; use --all, or pass --public-url \
             with the address your phone uses (for example from `tailscale serve`)"
        );
        return Ok(());
    };
    eprintln!(
        "herdr-companion: scan to open {} on your phone (treat the code like a password):\n{}",
        best.url,
        render_qr(&pairing_url(&best.url, token))?
    );
    for other in &urls[1..] {
        eprintln!("herdr-companion: also reachable at {}", other.url);
    }
    if !best.encrypted {
        eprintln!(
            "herdr-companion: warning: {} is plain HTTP, so anyone on that network can read the \
             token. A Tailscale address or `tailscale serve` encrypts it, and installing the app \
             to the home screen needs HTTPS.",
            best.url
        );
    }
    Ok(())
}
