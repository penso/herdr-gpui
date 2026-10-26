//! Starts the agent and wires the session, the attach socket, Herdr reports,
//! and the in-pane view together.

use std::{path::PathBuf, thread};

use anyhow::Context as _;
use crossbeam_channel::{Sender, bounded};
use herdr_chat_protocol::{EndpointId, NoticeLevel};

use crate::{
    agents::Launch,
    cli::{AgentChoice, Options},
    harness::{Acp, Pipes, Reach},
    herdr::{Reporter, Target},
    process,
    session::{Config, Flow, Input, Session, Update},
};

/// Inputs waiting for the session thread.
const INPUTS: usize = 1024;

/// Asks for the ACP adapter, as `--acp` does.
const ADAPTER_VARIABLE: &str = "HERDR_ACP_ADAPTER";

struct Agent {
    args: Vec<std::ffi::OsString>,
    display: String,
    login_hint: Option<String>,
    program: std::ffi::OsString,
    reach: Box<dyn Reach>,
}

fn agent(options: &Options) -> Agent {
    let name = options.name.clone();
    match &options.agent {
        AgentChoice::Builtin { agent, args: extra } => {
            let agent = *agent;
            let path = crate::agents::search_path(
                std::env::var_os("PATH").as_deref(),
                std::env::home_dir().as_deref(),
            );
            let adapter = options.adapter
                || std::env::var_os(ADAPTER_VARIABLE).is_some_and(|value| value == "1");
            let (program, mut args, reach): (_, _, Box<dyn Reach>) =
                match crate::agents::launch(agent, Some(&path), adapter) {
                    Launch::Native(program) => {
                        let (args, reach) = native(agent, options.load.as_deref());
                        (program.into_os_string(), args, reach)
                    }
                    Launch::Acp(program, args) => (program, args, Box::new(Acp)),
                };
            args.extend(extra.iter().cloned());
            Agent {
                args,
                display: name.unwrap_or_else(|| agent.display_name().to_owned()),
                login_hint: Some(agent.login_hint().to_owned()),
                program,
                reach,
            }
        }
        AgentChoice::Command { program, args } => Agent {
            display: name.unwrap_or_else(|| {
                PathBuf::from(&program)
                    .file_name()
                    .map_or_else(|| "Agent".to_owned(), |n| n.to_string_lossy().into_owned())
            }),
            args: args.clone(),
            login_hint: None,
            program: program.clone(),
            reach: Box::new(Acp),
        },
    }
}

/// The arguments that start `agent`'s own program, and the translator that
/// reaches it, continuing session `load` when set.
fn native(
    agent: crate::agents::Agent,
    load: Option<&str>,
) -> (Vec<std::ffi::OsString>, Box<dyn Reach>) {
    use crate::agents::Agent;
    match agent {
        Agent::Codex => (crate::codex::args(), crate::codex::reach()),
        Agent::Pi => (crate::pi::args(load), crate::pi::reach()),
        // `launch` reaches no other agent natively; Claude is the default.
        _ => {
            let session = load.map_or_else(|| uuid::Uuid::new_v4().to_string(), str::to_owned);
            (
                crate::claude::args(&session, load.is_some()),
                crate::claude::reach(session),
            )
        }
    }
}

pub(crate) fn run(options: Options) -> anyhow::Result<()> {
    let cwd = match options.cwd.clone() {
        Some(cwd) => cwd,
        None => std::env::current_dir().context("could not read the current directory")?,
    };
    let cwd = cwd
        .canonicalize()
        .with_context(|| format!("no directory {}", cwd.display()))?;
    let agent = agent(&options);
    let spawned = process::spawn(&agent.program, &agent.args, &cwd)?;
    let agent_process = spawned.process.clone();
    let (inputs, receiver) = bounded::<Input>(INPUTS);

    let to_session = inputs.clone();
    let outbox = agent.reach.connect(
        Pipes {
            input: Box::new(spawned.input),
            output: Box::new(spawned.output),
        },
        Box::new(move |inbound| to_session.send(Input::Agent(inbound)).is_ok()),
    )?;
    let waiter = inputs.clone();
    let watched = agent_process.clone();
    thread::Builder::new()
        .name("acp-wait".to_owned())
        .spawn(move || {
            let code = watched.wait().ok().and_then(|status| status.code());
            let stderr = watched.stderr_tail();
            let _ = waiter.send(Input::AgentExited { code, stderr });
        })?;

    let mut notices = Vec::new();
    // Held to the end: dropping the server removes its socket.
    let (_server, endpoint) = if options.attach {
        listen(inputs.clone(), &mut notices)
    } else {
        (None, None)
    };
    let target = Target::from_env();
    let reporter = match &target {
        Some(target) => Some(Reporter::start(
            target.clone(),
            agent.display.clone(),
            endpoint.map(|id| id.to_string()),
        )?),
        None => None,
    };
    let report = reporter.as_ref().map(Reporter::handle);

    let config = Config {
        agent: agent.display,
        cwd,
        exit_with_agent: options.headless,
        load: options.load,
        login_hint: agent.login_hint,
        pane: target.map(|target| target.pane),
    };
    let session_thread = thread::Builder::new()
        .name("acp-session".to_owned())
        .spawn(move || {
            let mut session = Session::new(config, outbox, move |update| {
                let Some(report) = &report else {
                    return;
                };
                match update {
                    Update::Activity(activity) => report.report(activity),
                    Update::Prompt(prompt) => report.prompt(prompt),
                }
            });
            for (level, text) in notices {
                session.notice(level, text);
            }
            session.start();
            for input in receiver {
                if session.handle(input) == Flow::Exit {
                    break;
                }
            }
        })?;

    // Headless, the session ends when the agent does; otherwise when the view quits.
    let result = if options.headless {
        Ok(())
    } else {
        let result = crate::tui::run(&inputs);
        let _ = inputs.send(Input::Shutdown);
        result
    };
    let _ = session_thread.join();
    agent_process.stop();
    if let Some(reporter) = reporter {
        reporter.close();
    }
    result
}

fn listen(
    inputs: Sender<Input>,
    notices: &mut Vec<(NoticeLevel, String)>,
) -> (Option<crate::attach::Server>, Option<EndpointId>) {
    let listening = crate::attach::address().and_then(|(address, id)| {
        crate::attach::Server::listen(&address, inputs).map(|server| (server, id))
    });
    match listening {
        Ok((server, id)) => (Some(server), Some(id)),
        Err(error) => {
            notices.push((
                NoticeLevel::Warning,
                format!("Herdr UI cannot attach: {:#}", anyhow::Error::from(error)),
            ));
            (None, None)
        }
    }
}
