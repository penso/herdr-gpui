//! The service definitions themselves, as text: a launchd property list, a
//! systemd unit, and a Task Scheduler task. Pure, so each platform's file is
//! tested on every platform.

use super::{LABEL, Plan};
use crate::app::{Error, Result};
use std::path::Path;

/// Where the daemon writes what it prints, beside its state.
const LOG: &str = "daemon.log";

/// The program and its arguments, each as text. A service file holds text,
/// so a path that is not UTF-8, or that holds a line break, is refused
/// rather than mangled.
fn words(plan: &Plan) -> Result<Vec<String>> {
    std::iter::once(plan.program.as_os_str().to_owned())
        .chain(plan.args())
        .map(|word| {
            word.to_str()
                .filter(|text| !text.contains(['\n', '\r', '\0']))
                .map(str::to_owned)
                .ok_or_else(|| Error::PathText { path: word.into() })
        })
        .collect()
}

fn path_text(path: &Path) -> Result<&str> {
    path.to_str()
        .filter(|text| !text.contains(['\n', '\r', '\0']))
        .ok_or_else(|| Error::PathText {
            path: path.to_owned(),
        })
}

/// Escapes text for an XML element's content.
fn xml(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            c => escaped.push(c),
        }
    }
    escaped
}

/// A macOS LaunchAgent: started at login and when loaded, restarted if it
/// crashes, and left stopped after it exits cleanly, as it does when Herdr
/// UI asks it to shut down.
pub fn launch_agent(plan: &Plan) -> Result<String> {
    let arguments: String = words(plan)?
        .iter()
        .map(|word| format!("\t\t<string>{}</string>\n", xml(word)))
        .collect();
    let log = xml(path_text(&plan.state_dir.join(LOG))?);
    Ok(format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \
         \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n\
         <dict>\n\
         \t<key>Label</key>\n\
         \t<string>{LABEL}</string>\n\
         \t<key>ProgramArguments</key>\n\
         \t<array>\n\
         {arguments}\
         \t</array>\n\
         \t<key>RunAtLoad</key>\n\
         \t<true/>\n\
         \t<key>KeepAlive</key>\n\
         \t<dict>\n\
         \t\t<key>SuccessfulExit</key>\n\
         \t\t<false/>\n\
         \t</dict>\n\
         \t<key>StandardOutPath</key>\n\
         \t<string>{log}</string>\n\
         \t<key>StandardErrorPath</key>\n\
         \t<string>{log}</string>\n\
         </dict>\n\
         </plist>\n"
    ))
}

/// One `ExecStart=` word: quoted, with backslashes and quotes escaped, `%`
/// doubled so it is not a specifier, and `$` doubled so it is not a
/// variable.
fn systemd_word(word: &str) -> String {
    let mut quoted = String::with_capacity(word.len() + 2);
    quoted.push('"');
    for c in word.chars() {
        match c {
            '\\' => quoted.push_str("\\\\"),
            '"' => quoted.push_str("\\\""),
            '%' => quoted.push_str("%%"),
            '$' => quoted.push_str("$$"),
            c => quoted.push(c),
        }
    }
    quoted.push('"');
    quoted
}

/// A systemd user unit, enabled for the user's default target. It restarts
/// only after a failure, so a requested shutdown stays down.
pub fn systemd_unit(plan: &Plan) -> Result<String> {
    let command: Vec<String> = words(plan)?.iter().map(|word| systemd_word(word)).collect();
    Ok(format!(
        "[Unit]\n\
         Description=Herdr remote control\n\
         \n\
         [Service]\n\
         ExecStart={}\n\
         Restart=on-failure\n\
         RestartSec=5\n\
         \n\
         [Install]\n\
         WantedBy=default.target\n",
        command.join(" ")
    ))
}

/// One argument as the C runtime's `CommandLineToArgvW` rules read it back:
/// bare when it has no space, tab, or quote; otherwise quoted, with quotes
/// escaped and the backslashes before them doubled.
pub fn windows_word(word: &str) -> String {
    if !word.is_empty() && !word.contains([' ', '\t', '"']) {
        return word.to_owned();
    }
    let mut quoted = String::with_capacity(word.len() + 2);
    quoted.push('"');
    let mut backslashes = 0;
    for c in word.chars() {
        match c {
            '\\' => backslashes += 1,
            '"' => {
                quoted.extend(std::iter::repeat_n('\\', backslashes * 2 + 1));
                quoted.push('"');
                backslashes = 0;
            }
            c => {
                quoted.extend(std::iter::repeat_n('\\', backslashes));
                quoted.push(c);
                backslashes = 0;
            }
        }
    }
    quoted.extend(std::iter::repeat_n('\\', backslashes * 2));
    quoted.push('"');
    quoted
}

/// A Task Scheduler task that starts the daemon when `user` signs in.
/// Written as XML rather than through `schtasks /TR` because that limits
/// the command to 261 characters, and because only XML can lift the
/// default three-day run limit and the battery rules that would stop it.
pub fn scheduled_task(plan: &Plan, user: &str) -> Result<String> {
    let mut words = words(plan)?.into_iter();
    let command = xml(&words.next().unwrap_or_default());
    let arguments = xml(&words
        .map(|word| windows_word(&word))
        .collect::<Vec<_>>()
        .join(" "));
    let user = xml(user);
    Ok(format!(
        "<?xml version=\"1.0\" encoding=\"UTF-16\"?>\r\n\
         <Task version=\"1.2\" xmlns=\"http://schemas.microsoft.com/windows/2004/02/mit/task\">\r\n\
         \x20 <RegistrationInfo>\r\n\
         \x20   <Description>Herdr remote control</Description>\r\n\
         \x20 </RegistrationInfo>\r\n\
         \x20 <Triggers>\r\n\
         \x20   <LogonTrigger>\r\n\
         \x20     <Enabled>true</Enabled>\r\n\
         \x20     <UserId>{user}</UserId>\r\n\
         \x20   </LogonTrigger>\r\n\
         \x20 </Triggers>\r\n\
         \x20 <Principals>\r\n\
         \x20   <Principal id=\"Author\">\r\n\
         \x20     <UserId>{user}</UserId>\r\n\
         \x20     <LogonType>InteractiveToken</LogonType>\r\n\
         \x20     <RunLevel>LeastPrivilege</RunLevel>\r\n\
         \x20   </Principal>\r\n\
         \x20 </Principals>\r\n\
         \x20 <Settings>\r\n\
         \x20   <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>\r\n\
         \x20   <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>\r\n\
         \x20   <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>\r\n\
         \x20   <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>\r\n\
         \x20   <RestartOnFailure>\r\n\
         \x20     <Interval>PT1M</Interval>\r\n\
         \x20     <Count>3</Count>\r\n\
         \x20   </RestartOnFailure>\r\n\
         \x20 </Settings>\r\n\
         \x20 <Actions Context=\"Author\">\r\n\
         \x20   <Exec>\r\n\
         \x20     <Command>{command}</Command>\r\n\
         \x20     <Arguments>{arguments}</Arguments>\r\n\
         \x20   </Exec>\r\n\
         \x20 </Actions>\r\n\
         </Task>\r\n"
    ))
}

/// The bytes `schtasks /XML` reads: UTF-16 little-endian with its mark, as
/// the declaration says.
pub fn utf16(text: &str) -> Vec<u8> {
    [0xFEFF_u16]
        .into_iter()
        .chain(text.encode_utf16())
        .flat_map(u16::to_le_bytes)
        .collect()
}
