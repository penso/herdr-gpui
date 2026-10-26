//! The agents `herdr-acp` starts by name, which clients such as Herdr UI
//! offer and pass on its command line. One table, so the program and its
//! clients cannot disagree about which names exist.

use std::{fmt, str::FromStr};

macro_rules! agents {
    ($($variant:ident => $name:literal, $display:literal, $login:literal, [$(($program:literal $(, $arg:literal)*)),+ $(,)?];)+) => {
        /// An agent `herdr-acp` starts by name.
        #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
        pub enum Agent {
            $($variant,)+
        }

        impl Agent {
            pub const ALL: &[Self] = &[$(Self::$variant,)+];

            pub fn name(self) -> &'static str {
                match self {
                    $(Self::$variant => $name,)+
                }
            }

            pub fn display_name(self) -> &'static str {
                match self {
                    $(Self::$variant => $display,)+
                }
            }

            /// How to sign in when the agent says it needs authentication.
            pub fn login_hint(self) -> &'static str {
                match self {
                    $(Self::$variant => $login,)+
                }
            }

            /// Commands that start the agent's ACP server, most direct first.
            pub fn candidates(self) -> &'static [(&'static str, &'static [&'static str])] {
                match self {
                    $(Self::$variant => &[$(($program, &[$($arg),*])),+],)+
                }
            }
        }
    };
}

agents! {
    // keep-sorted start
    Claude => "claude", "Claude", "run `claude` and use /login", [("claude-agent-acp"), ("npx", "-y", "@agentclientprotocol/claude-agent-acp")];
    Codex => "codex", "Codex", "run `codex login`", [("codex-acp"), ("npx", "-y", "@agentclientprotocol/codex-acp")];
    Copilot => "copilot", "Copilot", "run `copilot` and use /login, or for GitHub Enterprise `copilot login --host HOST` with GH_HOST set", [("copilot", "--acp")];
    Gemini => "gemini", "Gemini", "run `gemini` once to sign in", [("gemini", "--acp")];
    Opencode => "opencode", "OpenCode", "run `opencode auth login`", [("opencode", "acp")];
    Pi => "pi", "Pi", "run `pi` and use /login, or set your provider's API key", [("pi-acp"), ("npx", "-y", "pi-acp")];
    // keep-sorted end
}

/// A name that is not a built-in agent.
#[derive(Debug, Eq, PartialEq, thiserror::Error)]
#[error("unknown agent")]
pub struct UnknownAgent;

impl FromStr for Agent {
    type Err = UnknownAgent;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .iter()
            .copied()
            .find(|agent| agent.name() == name)
            .ok_or(UnknownAgent)
    }
}

impl fmt::Display for Agent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

impl Agent {
    /// Every name, comma-separated, for messages.
    pub fn names() -> String {
        Self::ALL
            .iter()
            .map(|agent| agent.name())
            .collect::<Vec<_>>()
            .join(", ")
    }
}
