//! A clear name for what listens on a port: a macOS app's bundle name, the
//! meaningful part of an interpreter's command line (`vite`, `next dev`,
//! `manage.py runserver`), or else the process's own name; and the project
//! its working directory belongs to. Pure functions over what the OS
//! reported, plus a few bounded reads of the file system the worker makes.

use std::path::{Path, PathBuf};

/// A derived name is display text; longer ones are cut.
const NAME_LIMIT: usize = 48;
/// Arguments after the script kept in a derived name (`next dev`).
const EXTRA_ARGUMENTS: usize = 2;
/// Folders walked up from a working directory looking for a repository.
const REPOSITORY_DEPTH: usize = 32;
/// An `Info.plist` larger than this is not read.
const PLIST_LIMIT: u64 = 256 * 1024;

/// Programs whose own name says little: what they run is the name.
const INTERPRETERS: &[&str] = &[
    "bun", "deno", "java", "node", "nodejs", "perl", "php", "python", "python2", "python3", "ruby",
    "tsx", "ts-node",
];

/// Listeners every computer of a kind runs, set apart so the user's own
/// servers stand out.
const SYSTEM: &[&str] = &[
    "ARDAgent",
    "AirPlayXPCHelper", // i18n: data
    "ControlCenter",    // i18n: data
    "UserEventAgent",   // i18n: data
    "avahi-daemon",
    "com.docker.backend",
    "com.docker.vpnkit",
    "cupsd",
    "launchd",
    "mDNSResponder",
    "rapportd",
    "remoted",
    "rpcbind",
    "sharingd",
    "sshd",
    "systemd",
    "systemd-resolved",
];

/// Whether `process` is one of the listeners every computer runs.
pub(crate) fn system(process: &str) -> bool {
    SYSTEM.contains(&process)
}

fn bounded(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control())
        .take(NAME_LIMIT)
        .collect::<String>()
        .trim()
        .to_owned()
}

fn base(argument: &str) -> &str {
    argument.rsplit('/').next().unwrap_or(argument)
}

/// `python3.12` and `node20` are their interpreters.
fn interpreter(program: &str) -> bool {
    let name = base(program).trim_end_matches(|c: char| c.is_ascii_digit() || c == '.');
    INTERPRETERS.contains(&name)
}

/// The meaningful part of an interpreter's command line: the script it
/// runs (as the tool a `node_modules` path names, or a `-m` module) and
/// the first arguments after it; `None` when `arguments` names no script.
pub(crate) fn command_name(arguments: &[String]) -> Option<String> {
    let (program, rest) = arguments.split_first()?;
    if !interpreter(program) {
        return None;
    }
    let mut words = rest.iter().map(String::as_str);
    let mut script = None;
    while let Some(word) = words.next() {
        match word {
            "-m" => {
                script = words.next().map(str::to_owned);
                break;
            }
            // Options taking a value, so the value is not the script.
            "-r" | "--require" | "--import" | "--loader" | "-cp" | "-classpath" | "-jar" => {
                let value = words.next();
                if word == "-jar" {
                    script = value.map(|jar| base(jar).to_owned());
                    break;
                }
            }
            "run" if base(program).starts_with("deno") || base(program).starts_with("bun") => {}
            _ if word.starts_with('-') => {}
            _ => {
                script = Some(script_name(word));
                break;
            }
        }
    }
    let mut name = script?;
    // Options after the script end what names it: `vite --port 5173`.
    for word in words
        .take_while(|word| !word.starts_with('-'))
        .take(EXTRA_ARGUMENTS)
    {
        name.push(' ');
        name.push_str(base(word));
    }
    Some(bounded(&name))
}

/// `node_modules/.bin/vite` and `node_modules/vite/bin/vite.js` are `vite`;
/// `npm-cli.js` is `npm`; anything else is its file name.
fn script_name(path: &str) -> String {
    let parts: Vec<&str> = path.split('/').collect();
    if let Some(index) = parts.iter().rposition(|part| *part == "node_modules") {
        if let Some(package) = parts.get(index + 1) {
            let package = if *package == ".bin" {
                parts.get(index + 2).copied().unwrap_or(package)
            } else if package.starts_with('@') {
                parts.get(index + 2).copied().unwrap_or(package)
            } else {
                package
            };
            return package.trim_end_matches(".js").to_owned();
        }
    }
    let file = base(path);
    match file {
        "npm-cli.js" => "npm".to_owned(),
        "npx-cli.js" => "npx".to_owned(),
        "yarn.js" => "yarn".to_owned(),
        "pnpm.cjs" | "pnpm.js" => "pnpm".to_owned(),
        _ => file.to_owned(),
    }
}

/// The outermost `*.app` bundle `executable` lives in.
pub(crate) fn app_bundle(executable: &Path) -> Option<&Path> {
    executable
        .ancestors()
        .filter(|path| path.extension().is_some_and(|extension| extension == "app"))
        .last()
}

/// A plain `<key>NAME</key><string>VALUE</string>` from an XML property
/// list; binary ones say nothing.
pub(crate) fn plist_string(plist: &str, key: &str) -> Option<String> {
    let marker = format!("<key>{key}</key>");
    let after = &plist[plist.find(&marker)? + marker.len()..];
    let value = after.trim_start().strip_prefix("<string>")?;
    let value = &value[..value.find("</string>")?];
    let value = value
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">");
    Some(bounded(&value)).filter(|value| !value.is_empty())
}

/// A bundle's display name from its `Info.plist`, else its folder's stem.
fn bundle_name(bundle: &Path) -> Option<String> {
    let plist = bundle.join("Contents/Info.plist"); // i18n: data
    let text = std::fs::metadata(&plist)
        .ok()
        .filter(|metadata| metadata.len() <= PLIST_LIMIT)
        .and_then(|_| std::fs::read_to_string(&plist).ok());
    text.as_deref()
        .and_then(|text| {
            plist_string(text, "CFBundleDisplayName").or_else(|| plist_string(text, "CFBundleName"))
        })
        .or_else(|| Some(bounded(&bundle.file_stem()?.to_string_lossy())))
}

/// What to call a process: its app, the script it runs, or its own name.
pub(crate) fn name(process: &str, executable: Option<&Path>, arguments: &[String]) -> String {
    executable
        .and_then(app_bundle)
        .and_then(bundle_name)
        .or_else(|| command_name(arguments))
        .unwrap_or_else(|| bounded(process))
}

/// The repository folder a working directory lies in, by its `.git`.
fn repository(directory: &Path) -> Option<PathBuf> {
    directory
        .ancestors()
        .take(REPOSITORY_DEPTH)
        .find(|folder| folder.join(".git").exists())
        .map(Path::to_path_buf)
}

/// The project a working directory belongs to: its repository's folder
/// name when it is inside one, else its own folder name.
pub(crate) fn project(directory: &Path) -> Option<String> {
    let root = repository(directory);
    let folder = root.as_deref().unwrap_or(directory);
    folder
        .file_name()
        .map(|name| bounded(&name.to_string_lossy()))
}
