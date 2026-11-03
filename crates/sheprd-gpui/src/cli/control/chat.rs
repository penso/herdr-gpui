//! `chat`: the native views of `herdr-acp` chats over their panes.
use super::{Args, CliError, Switch, unknown};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChatCommand {
    /// The window's chat panes and how each shows.
    Status,
    /// Open a tab running a chat with `agent`.
    New {
        agent: sheprd_chat_protocol::Agent,
        /// The pane whose workspace and folder it starts in, else the caller's.
        pane: Option<String>,
    },
    /// Show the chat view over a pane (`On`), or its terminal (`Off`).
    View {
        switch: Switch,
        /// The pane, else the caller's own.
        pane: Option<String>,
    },
}

pub(super) fn chat(args: &mut Args) -> Result<ChatCommand, CliError> {
    let Some(verb) = args.verb() else {
        return Ok(ChatCommand::Status);
    };
    let switch = match verb.to_str() {
        Some("status" | "list") => return Ok(ChatCommand::Status),
        Some("new") => {
            let pane = args.text_option("--pane")?;
            let agent = args
                .required_text("an agent, such as claude")?
                .parse()
                .map_err(|_| CliError::InvalidValue("AGENT"))?;
            return Ok(ChatCommand::New { agent, pane });
        }
        Some("show") => Switch::On,
        Some("hide") => Switch::Off,
        Some("toggle") => Switch::Toggle,
        _ => return Err(unknown("chat", verb)),
    };
    Ok(ChatCommand::View {
        switch,
        pane: args.text_option("--pane")?,
    })
}
