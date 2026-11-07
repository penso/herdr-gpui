//! Coding agents a new cloud device can install before the user's own setup
//! script. Each install skips a tool already on the machine, so running the
//! setup again on a device installs only what is missing. An installer is
//! downloaded before it runs: `curl | bash` succeeds when the download fails,
//! because `bash` exits cleanly on empty input, while a failed assignment
//! stops the setup's `sh -e`.

/// Declares every agent once: its config key, its label, and its install
/// lines. The order is the order Settings shows and the setup installs.
macro_rules! agents {
    ($($variant:ident => $key:literal, $label:literal, $script:expr;)+) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub(crate) enum Agent {
            $($variant,)+
        }

        impl Agent {
            pub(crate) const ALL: &[Self] = &[$(Self::$variant,)+];

            /// The name `agents` lists in `[cloud]`.
            pub(crate) fn key(self) -> &'static str {
                match self {
                    $(Self::$variant => $key,)+
                }
            }

            #[cfg(any(test, feature = "cloud"))]
            pub(crate) fn label(self) -> &'static str {
                match self {
                    $(Self::$variant => $label,)+
                }
            }

            /// Shell lines that install the agent unless it is already there.
            pub(crate) fn script(self) -> &'static str {
                match self {
                    $(Self::$variant => $script,)+
                }
            }

            pub(crate) fn from_key(key: &str) -> Option<Self> {
                Self::ALL.iter().copied().find(|agent| agent.key() == key)
            }
        }
    };
}

agents! {
    Claude => "claude", "Claude Code", // i18n: data
        // i18n: data
        "echo '==> Installing Claude Code'\n\
         if ! command -v claude >/dev/null 2>&1; then \
         installer=$(curl -fsSL https://claude.ai/install.sh); \
         printf '%s\\n' \"$installer\" | bash; fi";
    Codex => "codex", "Codex", // i18n: data
        // i18n: data
        "echo '==> Installing Codex'\n\
         if command -v codex >/dev/null 2>&1; then :; \
         elif command -v npm >/dev/null 2>&1; then npm install -g @openai/codex; \
         else echo 'Skipping Codex: it needs npm (Node.js)' >&2; fi";
    Gemini => "gemini", "Gemini CLI", // i18n: data
        // i18n: data
        "echo '==> Installing Gemini CLI'\n\
         if command -v gemini >/dev/null 2>&1; then :; \
         elif command -v npm >/dev/null 2>&1; then npm install -g @google/gemini-cli; \
         else echo 'Skipping Gemini CLI: it needs npm (Node.js)' >&2; fi";
    OpenCode => "opencode", "OpenCode", // i18n: data
        // i18n: data
        "echo '==> Installing OpenCode'\n\
         if ! command -v opencode >/dev/null 2>&1; then \
         installer=$(curl -fsSL https://opencode.ai/install); \
         printf '%s\\n' \"$installer\" | bash; fi";
}
