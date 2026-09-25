//! What a provider reads its sign-in and its usage through. The same calls
//! work on this machine and on a remote host: there, every call runs in one
//! SSH shell session, and a secret read on the host stays in that shell as a
//! variable. Only a reference to it crosses back, and HTTP requests that use
//! it run on the host with `curl`, so credentials never leave the machine
//! they belong to. Settings from this machine's config stay here, and a
//! request using them runs here whichever host is selected.

use super::{cookies::CookieJar, model::Provider, service::Setting, settings::ProviderSettings};
use crate::{Error, Result};
use secrecy::{ExposeSecret, SecretString};
use std::{
    io::{Read, Write},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

/// Responses and files larger than this are refused rather than truncated.
pub(super) const LIMIT: usize = 1024 * 1024;
const HTTP_TIMEOUT: Duration = Duration::from_secs(15);
const STEP_TIMEOUT: Duration = Duration::from_secs(20);
#[cfg(unix)]
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);

/// A credential, held where it was read.
#[derive(Clone)]
pub(crate) struct Secret(Held);

#[derive(Clone)]
enum Held {
    Here(SecretString),
    /// The name of a shell variable in the remote session.
    There(String),
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self.0 {
            Held::Here(_) => "Secret(here)",
            Held::There(_) => "Secret(remote)",
        })
    }
}

impl From<SecretString> for Secret {
    fn from(value: SecretString) -> Self {
        Self(Held::Here(value))
    }
}

/// A file on the probed host. `~/` is that host's home, and a base may come
/// from an environment variable there, as agents' config directories do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HostPath {
    variable: Option<&'static str>,
    /// Relative to the home directory unless absolute.
    base: &'static str,
    rest: String,
}

impl HostPath {
    /// `~/.codex/auth.json` is `HostPath::home(".codex/auth.json")`.
    pub fn home(path: impl Into<String>) -> Self {
        Self {
            variable: None,
            base: "",
            rest: path.into(),
        }
    }

    /// `$CODEX_HOME`, or `~/.codex` when it is unset, then `rest`.
    pub fn env_or(
        variable: &'static str,
        home_relative: &'static str,
        rest: impl Into<String>,
    ) -> Self {
        Self {
            variable: Some(variable),
            base: home_relative,
            rest: rest.into(),
        }
    }

    pub fn absolute(path: impl Into<String>) -> Self {
        Self {
            variable: None,
            base: "/",
            rest: path.into().trim_start_matches('/').to_owned(),
        }
    }

    fn local(&self) -> Option<std::path::PathBuf> {
        let base = match (self.variable, self.base) {
            (Some(variable), base) => std::env::var_os(variable)
                .filter(|value| !value.is_empty())
                .map(std::path::PathBuf::from)
                .or_else(|| crate::config::home().ok().map(|home| home.join(base)))?,
            (None, "/") => std::path::PathBuf::from("/"),
            (None, base) => crate::config::home().ok()?.join(base),
        };
        Some(if self.rest.is_empty() {
            base
        } else {
            base.join(&self.rest)
        })
    }

    /// A shell word that expands to the path on the remote host.
    fn remote(&self) -> String {
        let base = match (self.variable, self.base) {
            (Some(variable), base) => {
                format!("\"${{{variable}:-$HOME/{}}}\"", base.replace('"', ""))
            }
            (None, "/") => String::new(),
            (None, "") => "\"$HOME\"".into(),
            (None, base) => format!("\"$HOME\"/{}", quote(base)),
        };
        match (base.is_empty(), self.rest.is_empty()) {
            (true, _) => quote(&format!("/{}", self.rest)),
            (false, true) => base,
            (false, false) => format!("{base}/{}", quote(&self.rest)),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Method {
    Get,
    Post,
}

/// A piece of a header or body: literal text, or a secret spliced in where
/// the secret lives.
#[derive(Clone, Debug)]
pub(crate) enum Part {
    Text(String),
    Secret(Secret),
}

#[derive(Clone, Debug)]
pub(crate) struct Request {
    pub method: Method,
    pub url: String,
    pub headers: Vec<(String, Vec<Part>)>,
    pub body: Option<Vec<Part>>,
    pub timeout: Duration,
}

impl Request {
    pub fn get(url: impl Into<String>) -> Self {
        Self {
            method: Method::Get,
            url: url.into(),
            headers: Vec::new(),
            body: None,
            timeout: HTTP_TIMEOUT,
        }
    }

    pub fn post(url: impl Into<String>) -> Self {
        Self {
            method: Method::Post,
            ..Self::get(url)
        }
    }

    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers
            .push((name.into(), vec![Part::Text(value.into())]));
        self
    }

    /// `name: <prefix><secret>`, e.g. `bearer` is `Authorization: Bearer <secret>`.
    pub fn secret_header(mut self, name: impl Into<String>, prefix: &str, secret: &Secret) -> Self {
        let mut parts = Vec::new();
        if !prefix.is_empty() {
            parts.push(Part::Text(prefix.to_owned()));
        }
        parts.push(Part::Secret(secret.clone()));
        self.headers.push((name.into(), parts));
        self
    }

    pub fn bearer(self, token: &Secret) -> Self {
        self.secret_header("Authorization", "Bearer ", token)
    }

    pub fn cookie(self, cookies: &Secret) -> Self {
        self.secret_header("Cookie", "", cookies)
    }

    /// A JSON body with no secrets in it.
    pub fn json(self, body: impl Into<String>) -> Self {
        self.header("Content-Type", "application/json")
            .body(vec![Part::Text(body.into())])
    }

    pub fn body(mut self, parts: Vec<Part>) -> Self {
        self.body = Some(parts);
        self
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    fn secrets(&self) -> impl Iterator<Item = &Secret> {
        self.headers
            .iter()
            .flat_map(|(_, parts)| parts)
            .chain(self.body.iter().flatten())
            .filter_map(|part| match part {
                Part::Secret(secret) => Some(secret),
                Part::Text(_) => None,
            })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Response {
    /// 0 when no answer arrived.
    pub status: u16,
    pub body: String,
}

impl Response {
    /// The body when the status is success, else the matching usage error.
    pub fn ok(self) -> Result<String> {
        match self.status {
            200..=299 => Ok(self.body),
            0 => Err(Error::UsageConnect),
            401 | 403 => Err(Error::UsageRejected),
            429 => Err(Error::UsageRateLimited),
            status => Err(Error::UsageStatus(status)),
        }
    }

    pub fn json<T: serde::de::DeserializeOwned>(self) -> Result<T> {
        super::service::json(&self.ok()?)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Output {
    pub success: bool,
    pub stdout: String,
}

/// Where the probe runs: this machine, or a shell on a remote host.
pub(super) enum Exec {
    Local,
    Remote(Shell),
}

pub(crate) struct Probe<'a> {
    exec: &'a mut Exec,
    provider: Provider,
    settings: Option<&'a ProviderSettings>,
    cookies: &'a mut CookieJar,
    /// Whether the config asked for this provider, which is what allows
    /// reading browser cookies for it.
    requested: bool,
}

impl<'a> Probe<'a> {
    pub(super) fn new(
        exec: &'a mut Exec,
        provider: Provider,
        settings: Option<&'a ProviderSettings>,
        cookies: &'a mut CookieJar,
        requested: bool,
    ) -> Self {
        Self {
            exec,
            provider,
            settings,
            cookies,
            requested,
        }
    }

    pub fn is_remote(&self) -> bool {
        matches!(self.exec, Exec::Remote(_))
    }

    /// Whether the probed host is a Mac, where agents keep sign-ins in the
    /// login keychain.
    pub fn is_macos(&mut self) -> bool {
        match self.exec {
            Exec::Local => cfg!(target_os = "macos"),
            Exec::Remote(shell) => shell.macos(),
        }
    }

    /// A declared setting from this machine's config, else from the first of
    /// its environment variables set for this app.
    pub fn setting(&self, name: &str) -> Option<Secret> {
        self.settings
            .and_then(|settings| settings.get(name))
            .cloned()
            .or_else(|| {
                let setting = self.declared(name)?;
                setting.env.iter().find_map(|variable| {
                    std::env::var(variable)
                        .ok()
                        .filter(|value| !value.trim().is_empty())
                        .map(SecretString::from)
                })
            })
            .map(Secret::from)
    }

    /// A setting that is not a secret, such as a base URL or a team id.
    pub fn text_setting(&self, name: &str) -> Option<String> {
        match self.setting(name)?.0 {
            Held::Here(value) => Some(value.expose_secret().trim().to_owned()),
            Held::There(_) => None,
        }
    }

    fn declared(&self, name: &str) -> Option<&'static Setting> {
        self.provider
            .service()
            .meta()
            .settings
            .iter()
            .find(|setting| setting.name == name)
    }

    /// An environment variable on the probed host.
    pub fn env(&mut self, name: &str) -> Option<Secret> {
        match self.exec {
            Exec::Local => std::env::var(name)
                .ok()
                .filter(|value| !value.is_empty())
                .map(|value| Secret::from(SecretString::from(value))),
            Exec::Remote(shell) => shell.capture(&format!("printenv {}", quote(name))),
        }
    }

    /// A whole file on the probed host, kept as a secret. Trailing newlines
    /// are dropped, as the remote shell's `$(…)` drops them, so a token file
    /// can go straight into a header.
    pub fn file(&mut self, path: &HostPath) -> Option<Secret> {
        match self.exec {
            Exec::Local => read_local(&path.local()?)
                .and_then(|bytes| String::from_utf8(bytes.to_vec()).ok())
                .map(|text| text.trim_end_matches(['\n', '\r']).to_owned())
                .filter(|text| !text.is_empty())
                .map(|text| Secret::from(SecretString::from(text))),
            Exec::Remote(shell) => shell.capture(&format!("cat -- {}", path.remote())),
        }
    }

    pub fn exists(&mut self, path: &HostPath) -> bool {
        match self.exec {
            Exec::Local => path.local().is_some_and(|path| path.exists()),
            Exec::Remote(shell) => shell
                .run(&format!("[ -e {} ]", path.remote()), STEP_TIMEOUT)
                .is_ok_and(|output| output.success),
        }
    }

    /// A file that holds nothing secret, such as a quota report, brought back.
    pub fn read(&mut self, path: &HostPath) -> Option<String> {
        match self.exec {
            Exec::Local => {
                read_local(&path.local()?).and_then(|bytes| String::from_utf8(bytes.to_vec()).ok())
            }
            Exec::Remote(shell) => shell
                .run(&format!("cat -- {}", path.remote()), STEP_TIMEOUT)
                .ok()
                .filter(|output| output.success)
                .map(|output| output.stdout),
        }
    }

    /// A macOS keychain item's password on the probed host.
    /// A macOS keychain generic password on the probed host (`security -w`).
    pub fn keychain(&mut self, service: &str, account: Option<&str>) -> Option<Secret> {
        self.security("find-generic-password", service, account)
    }

    /// A macOS keychain internet password, keyed by server, as some editors
    /// store their sign-in.
    pub fn keychain_internet(&mut self, server: &str, account: Option<&str>) -> Option<Secret> {
        self.security("find-internet-password", server, account)
    }

    fn security(&mut self, kind: &str, service: &str, account: Option<&str>) -> Option<Secret> {
        let mut args = vec![kind, "-s", service, "-w"];
        if let Some(account) = account {
            args.extend(["-a", account]);
        }
        match self.exec {
            Exec::Local => {
                if !cfg!(target_os = "macos") {
                    return None;
                }
                let mut command = Command::new("/usr/bin/security");
                command.args(&args);
                let (success, bytes) =
                    output(&mut command, STEP_TIMEOUT, "read a keychain item").ok()?;
                let text = String::from_utf8(bytes.to_vec()).ok()?;
                let text = text.trim_end_matches('\n');
                (success && !text.is_empty())
                    .then(|| Secret::from(SecretString::from(text.to_owned())))
            }
            Exec::Remote(shell) => shell.capture(&format!(
                "security {} 2>/dev/null",
                args.iter()
                    .map(|arg| quote(arg))
                    .collect::<Vec<_>>()
                    .join(" ")
            )),
        }
    }

    /// A string or number at `path` inside the JSON `source`, kept as a secret.
    pub fn field(&mut self, source: &Secret, path: &[&str]) -> Option<Secret> {
        match (&mut *self.exec, &source.0) {
            (_, Held::Here(text)) => json_field(text.expose_secret(), path)
                .map(|value| Secret::from(SecretString::from(value))),
            (Exec::Remote(shell), Held::There(variable)) => shell.capture(&format!(
                "printf '%s' \"${variable}\" | herdr_field {}",
                path.iter()
                    .map(|key| quote(key))
                    .collect::<Vec<_>>()
                    .join(" ")
            )),
            (Exec::Local, Held::There(_)) => None,
        }
    }

    /// A field that is not secret, such as a plan name or an email, revealed.
    pub fn text(&mut self, source: &Secret, path: &[&str]) -> Option<String> {
        let field = self.field(source, path)?;
        self.reveal(&field)
    }

    /// A non-secret JSON field of a file, without bringing the file back.
    pub fn file_text(&mut self, path: &HostPath, field: &[&str]) -> Option<String> {
        let file = self.file(path)?;
        self.text(&file, field)
    }

    fn reveal(&mut self, secret: &Secret) -> Option<String> {
        let text = match (&mut *self.exec, &secret.0) {
            (_, Held::Here(value)) => value.expose_secret().to_owned(),
            (Exec::Remote(shell), Held::There(variable)) => {
                shell
                    .run(&format!("printf '%s' \"${variable}\""), STEP_TIMEOUT)
                    .ok()
                    .filter(|output| output.success)?
                    .stdout
            }
            (Exec::Local, Held::There(_)) => return None,
        };
        let text = text.trim();
        (!text.is_empty()).then(|| text.to_owned())
    }

    /// A command on the probed host, with the usual per-user install
    /// directories on its PATH. Its output is brought back, so it must not
    /// print secrets.
    pub fn command(&mut self, program: &str, args: &[&str], timeout: Duration) -> Result<Output> {
        match self.exec {
            Exec::Local => {
                let mut command = Command::new(program);
                command
                    .args(args)
                    .env("PATH", local_path())
                    .env("NO_COLOR", "1");
                let (success, bytes) = output(&mut command, timeout, "run a provider command")?;
                Ok(Output {
                    success,
                    stdout: String::from_utf8_lossy(&bytes).into_owned(),
                })
            }
            Exec::Remote(shell) => shell.run(
                &format!(
                    "NO_COLOR=1 {} {} </dev/null 2>/dev/null",
                    quote(program),
                    args.iter()
                        .map(|arg| quote(arg))
                        .collect::<Vec<_>>()
                        .join(" ")
                ),
                timeout,
            ),
        }
    }

    /// The Cookie header for a web sign-in: the `cookie` setting when set,
    /// else, on this machine and only for providers the config asked for,
    /// the named cookies for `domains` from Chrome or Safari.
    pub fn cookies(&mut self, domains: &[&str], names: &[&str]) -> Option<Secret> {
        if let Some(cookie) = self.setting("cookie") {
            return Some(cookie);
        }
        if !self.requested {
            return None;
        }
        self.cookies
            .header(domains, names)
            .map(|header| Secret::from(SecretString::from(header)))
    }

    /// Like [`Probe::cookies`], but satisfied by whichever of `names` the
    /// browser has, for services that renamed their session cookie.
    pub fn cookies_any(&mut self, domains: &[&str], names: &[&str]) -> Option<Secret> {
        if let Some(cookie) = self.setting("cookie") {
            return Some(cookie);
        }
        if !self.requested {
            return None;
        }
        names.iter().find_map(|name| {
            self.cookies
                .header(domains, &[name])
                .map(|header| Secret::from(SecretString::from(header)))
        })
    }

    /// One cookie's bare value, for services that want it as a bearer token
    /// or echoed in a header: from the `cookie` setting's header, else from
    /// the browser as [`Probe::cookies`] reads it.
    pub fn cookie_value(&mut self, domains: &[&str], name: &str) -> Option<Secret> {
        let header = match self.setting("cookie") {
            Some(Secret(Held::Here(header))) => Zeroizing::new(header.expose_secret().to_owned()),
            Some(Secret(Held::There(_))) => return None,
            None if self.requested => Zeroizing::new(self.cookies.header(domains, &[name])?),
            None => return None,
        };
        cookie_in(&header, name).map(|value| Secret::from(SecretString::from(value)))
    }

    /// A host environment variable that is not secret, such as a local
    /// server address.
    pub fn env_text(&mut self, name: &str) -> Option<String> {
        let value = self.env(name)?;
        self.reveal(&value)
    }

    pub fn http(&mut self, request: Request) -> Result<Response> {
        match self.place(&request)? {
            Place::Here => http_local(&request),
            Place::There => match self.exec {
                Exec::Remote(shell) => shell.http(&request),
                Exec::Local => Err(Error::UsageMixedSecrets),
            },
        }
    }

    /// The body of a successful answer; a failed one becomes its usage
    /// error, as [`Response::ok`] maps it.
    pub fn body(&mut self, request: Request) -> Result<String> {
        self.http(request)?.ok()
    }

    /// Exchanges one credential for another, such as a short-lived API token,
    /// keeping the new one where the request ran.
    pub fn exchange(&mut self, request: Request, path: &[&str]) -> Result<Secret> {
        match self.place(&request)? {
            Place::Here => {
                let body = http_local(&request)?.ok()?;
                json_field(&body, path)
                    .map(|value| Secret::from(SecretString::from(value)))
                    .ok_or(Error::UsageJson(serde_json::error::Category::Data))
            }
            Place::There => match self.exec {
                Exec::Remote(shell) => shell.exchange(&request, path),
                Exec::Local => Err(Error::UsageMixedSecrets),
            },
        }
    }

    /// A request runs where its secrets are: remote secrets on the host,
    /// this machine's settings here, and one without secrets on the host so
    /// it sees what the host sees.
    fn place(&self, request: &Request) -> Result<Place> {
        let (mut here, mut there) = (false, false);
        for secret in request.secrets() {
            match secret.0 {
                Held::Here(_) => here = true,
                Held::There(_) => there = true,
            }
        }
        match (here, there, self.is_remote()) {
            (true, true, _) => Err(Error::UsageMixedSecrets),
            (true, false, _) | (false, _, false) => Ok(Place::Here),
            (false, _, true) => Ok(Place::There),
        }
    }
}

enum Place {
    Here,
    There,
}

/// Agents install into per-user directories an app launched from the Dock
/// does not have on its PATH.
fn local_path() -> String {
    let home = crate::config::home()
        .map(|home| home.display().to_string())
        .unwrap_or_default();
    let mut path = format!(
        "{home}/.local/bin:{home}/.cargo/bin:{home}/.bun/bin:{home}/.npm-global/bin:/opt/homebrew/bin:/usr/local/bin"
    );
    if let Some(inherited) = std::env::var_os("PATH") {
        path.push(':');
        path.push_str(&inherited.to_string_lossy());
    }
    path
}

fn read_local(path: &std::path::Path) -> Option<Zeroizing<Vec<u8>>> {
    let mut bytes = Zeroizing::new(Vec::new());
    std::fs::File::open(path)
        .ok()?
        .take(LIMIT as u64 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    (bytes.len() <= LIMIT).then_some(bytes)
}

/// The value of cookie `name` in a `name=value; …` header.
pub(super) fn cookie_in(header: &str, name: &str) -> Option<String> {
    header.split(';').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        (key.trim() == name)
            .then(|| value.trim().to_owned())
            .filter(|value| !value.is_empty())
    })
}

/// The string or number at `path`; array steps are indices.
pub(super) fn json_field(text: &str, path: &[&str]) -> Option<String> {
    let mut value: &serde_json::Value = &serde_json::from_str(text).ok()?;
    for key in path {
        value = match value {
            serde_json::Value::Array(items) => items.get(key.parse::<usize>().ok()?)?,
            _ => value.get(key)?,
        };
    }
    let root = match value {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Number(number) => number.to_string(),
        serde_json::Value::Bool(flag) => flag.to_string(),
        _ => return None,
    };
    (!root.is_empty()).then_some(root)
}

fn text_of(parts: &[Part]) -> Option<Zeroizing<String>> {
    let mut text = Zeroizing::new(String::new());
    for part in parts {
        match part {
            Part::Text(value) => text.push_str(value),
            Part::Secret(Secret(Held::Here(value))) => text.push_str(value.expose_secret()),
            Part::Secret(Secret(Held::There(_))) => return None,
        }
    }
    Some(text)
}

fn http_local(request: &Request) -> Result<Response> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(request.timeout))
        .max_redirects(0)
        .http_status_as_error(false)
        .build()
        .into();
    let mut headers = Vec::with_capacity(request.headers.len());
    for (name, parts) in &request.headers {
        let text = text_of(parts).ok_or(Error::UsageMixedSecrets)?;
        let mut value = ureq::http::HeaderValue::from_str(&text).map_err(Error::UsageHeader)?;
        value.set_sensitive(parts.iter().any(|part| matches!(part, Part::Secret(_))));
        headers.push((name.as_str(), value));
    }
    let result = match request.method {
        Method::Get => {
            let mut call = agent.get(&request.url);
            for (name, value) in headers {
                call = call.header(name, value);
            }
            call.call()
        }
        Method::Post => {
            let mut call = agent.post(&request.url);
            for (name, value) in headers {
                call = call.header(name, value);
            }
            let body = match &request.body {
                Some(parts) => text_of(parts).ok_or(Error::UsageMixedSecrets)?,
                None => Zeroizing::new(String::new()),
            };
            call.send(body.as_bytes())
        }
    };
    let mut response = result.map_err(Error::UsageNetwork)?;
    let status = response.status().as_u16();
    let mut body = String::new();
    response
        .body_mut()
        .as_reader()
        .take(LIMIT as u64 + 1)
        .read_to_string(&mut body)
        .map_err(|source| Error::UsageNetwork(ureq::Error::Io(source)))?;
    if body.len() > LIMIT {
        return Err(Error::UsageSize);
    }
    Ok(Response { status, body })
}

/// `'text'`, safe as one POSIX shell word.
pub(super) fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// Helpers defined once per remote session. `herdr_field` prints the string
/// or number at a JSON path from stdin, preferring a real parser and falling
/// back to matching the last key when the host has neither. Remote sessions
/// need an `ssh` child, which only Unix clients start.
#[cfg(unix)]
const PRELUDE: &str = r#"PATH="$HOME/.local/bin:$HOME/.cargo/bin:$HOME/.bun/bin:$HOME/.npm-global/bin:/opt/homebrew/bin:/usr/local/bin:$PATH"
export PATH
herdr_field() {
    if command -v python3 >/dev/null 2>&1; then
        python3 -c 'import json,sys
v=json.load(sys.stdin)
for k in sys.argv[1:]:
    v=v[int(k)] if isinstance(v,list) else v[k]
if isinstance(v,bool): v=str(v).lower()
if v is None or isinstance(v,(dict,list)): sys.exit(1)
sys.stdout.write(str(v))' "$@" 2>/dev/null
    elif command -v jq >/dev/null 2>&1; then
        herdr_path=
        for herdr_key in "$@"; do
            case "$herdr_key" in *[!0-9]*) herdr_path="$herdr_path[\"$herdr_key\"]";; *) herdr_path="$herdr_path[$herdr_key]";; esac
        done
        jq -j "$herdr_path // empty" 2>/dev/null
    else
        for herdr_last in "$@"; do :; done
        sed -n "s/.*\"$herdr_last\"[[:space:]]*:[[:space:]]*\"\{0,1\}\([^\",}]*\)\"\{0,1\}.*/\1/p" | head -n 1 | tr -d '\n'
    fi
}
"#;

/// A `/bin/sh` on a remote host, fed one step at a time over SSH stdin. Each
/// step ends with a marker carrying its exit status, so steps can be read
/// back without closing the session.
pub(super) struct Shell {
    child: Child,
    stdin: ChildStdin,
    output: mpsc::Receiver<std::io::Result<Vec<u8>>>,
    buffer: Vec<u8>,
    variables: usize,
    marker: String,
    macos: Option<bool>,
    broken: bool,
}

impl Drop for Shell {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Shell {
    #[cfg(unix)]
    pub fn connect(target: &str) -> Result<Self> {
        let mut command = herdr_client::script_command(target, "exec /bin/sh -s")?;
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        Self::start(command)
    }

    #[cfg(windows)]
    pub fn connect(_target: &str) -> Result<Self> {
        Err(Error::UsageUnsupported)
    }

    /// Any `sh` reading steps from stdin; SSH in production, a local shell
    /// in tests.
    #[cfg(unix)]
    pub fn start(mut command: Command) -> Result<Self> {
        let process = |source| Error::UsageProcess {
            operation: "start the remote usage shell",
            source,
        };
        let mut child = command.spawn().map_err(process)?;
        let (Some(stdin), Some(mut stdout)) = (child.stdin.take(), child.stdout.take()) else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(process(std::io::Error::other("no pipes")));
        };
        let (sender, output) = mpsc::sync_channel(64);
        let reader = thread::Builder::new()
            .name("herdr-usage-shell".into())
            .spawn(move || {
                let mut chunk = [0; 8192];
                loop {
                    match stdout.read(&mut chunk) {
                        Ok(0) => break,
                        Ok(n) => {
                            if sender.send(Ok(chunk[..n].to_vec())).is_err() {
                                break;
                            }
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                        Err(error) => {
                            let _ = sender.send(Err(error));
                            break;
                        }
                    }
                }
            });
        if let Err(source) = reader {
            let _ = child.kill();
            let _ = child.wait();
            return Err(process(source));
        }
        let mut shell = Self {
            child,
            stdin,
            output,
            buffer: Vec::new(),
            variables: 0,
            marker: format!("@@herdr-{}", uuid::Uuid::new_v4().simple()),
            macos: None,
            broken: false,
        };
        shell.write(PRELUDE)?;
        let ready = shell.run("true", CONNECT_TIMEOUT)?;
        if !ready.success {
            return Err(Error::UsageUnreachable);
        }
        Ok(shell)
    }

    fn write(&mut self, text: &str) -> Result<()> {
        self.stdin
            .write_all(text.as_bytes())
            .and_then(|()| self.stdin.flush())
            .map_err(|_| {
                self.broken = true;
                Error::UsageUnreachable
            })
    }

    fn macos(&mut self) -> bool {
        if self.macos.is_none() {
            self.macos = Some(
                self.run("uname -s", STEP_TIMEOUT)
                    .is_ok_and(|output| output.stdout.trim() == "Darwin"),
            );
        }
        self.macos.unwrap_or(false)
    }

    /// Runs `step` and returns what it printed. A step that overruns breaks
    /// the session, since its output could still arrive later.
    pub fn run(&mut self, step: &str, timeout: Duration) -> Result<Output> {
        if self.broken {
            return Err(Error::UsageUnreachable);
        }
        let marker = self.marker.clone();
        self.write(&format!(
            "{{ {step}\n}} </dev/null; printf '\\n{marker} %s\\n' \"$?\"\n"
        ))?;
        let deadline = Instant::now() + timeout;
        let end = format!("\n{marker} ");
        loop {
            if let Some(start) = find(&self.buffer, end.as_bytes()) {
                let tail = start + end.len();
                if let Some(newline) = self.buffer[tail..].iter().position(|b| *b == b'\n') {
                    let status = String::from_utf8_lossy(&self.buffer[tail..tail + newline])
                        .trim()
                        .parse::<i32>()
                        .unwrap_or(1);
                    let stdout = String::from_utf8_lossy(&self.buffer[..start]).into_owned();
                    self.buffer.drain(..tail + newline + 1);
                    return Ok(Output {
                        success: status == 0,
                        stdout,
                    });
                }
            }
            if self.buffer.len() > LIMIT {
                self.broken = true;
                return Err(Error::UsageSize);
            }
            let left = deadline.saturating_duration_since(Instant::now());
            match self.output.recv_timeout(left) {
                Ok(Ok(chunk)) => self.buffer.extend_from_slice(&chunk),
                Ok(Err(_)) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                    self.broken = true;
                    return Err(Error::UsageUnreachable);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    self.broken = true;
                    return Err(Error::UsageTimeout);
                }
            }
        }
    }

    /// Assigns what `producer` prints to a new variable and hands back a
    /// reference to it; None when it printed nothing or failed.
    fn capture(&mut self, producer: &str) -> Option<Secret> {
        self.variables += 1;
        let variable = format!("herdr_s{}", self.variables);
        let output = self
            .run(
                &format!("{variable}=$({producer} 2>/dev/null) && [ -n \"${variable}\" ]"),
                STEP_TIMEOUT,
            )
            .ok()?;
        output.success.then(|| Secret(Held::There(variable)))
    }

    /// `curl -K` reads the request from a here-document, so secrets expand
    /// inside the host's shell and never appear in an argument list. `flags`
    /// go before the config and `pipe` after it on the same line, as a
    /// here-document requires.
    fn curl(request: &Request, flags: &str, pipe: &str) -> Option<String> {
        if request.url.contains(['\n', '"']) {
            return None;
        }
        let mut config = format!("url = \"{}\"\n", escape(&request.url));
        if request.method == Method::Post {
            config.push_str("request = \"POST\"\n");
        }
        for (name, parts) in &request.headers {
            config.push_str(&format!(
                "header = \"{}: {}\"\n",
                escape(name),
                splice(parts)?
            ));
        }
        if let Some(body) = &request.body {
            config.push_str(&format!("data-raw = \"{}\"\n", splice(body)?));
        }
        let limit = request.timeout.as_secs().max(1);
        Some(format!(
            "curl -sS --max-time {limit} {flags} -K /dev/fd/3 3<<@@herdr-curl {pipe}\n{config}@@herdr-curl\n"
        ))
    }

    fn http(&mut self, request: &Request) -> Result<Response> {
        let curl = Self::curl(request, "-w '\\n@@herdr-status %{http_code}'", "")
            .ok_or(Error::UsageMixedSecrets)?;
        let output = self.run(
            &format!("command -v curl >/dev/null 2>&1 || exit 127\n{curl}"),
            request.timeout + Duration::from_secs(5),
        )?;
        let Some((body, status)) = output.stdout.rsplit_once("\n@@herdr-status ") else {
            return Err(if output.stdout.is_empty() && !output.success {
                Error::UsageMissingCurl
            } else {
                Error::UsageConnect
            });
        };
        Ok(Response {
            status: status.trim().parse().unwrap_or(0),
            body: body.to_owned(),
        })
    }

    fn exchange(&mut self, request: &Request, path: &[&str]) -> Result<Secret> {
        let keys = path
            .iter()
            .map(|key| quote(key))
            .collect::<Vec<_>>()
            .join(" ");
        let curl = Self::curl(request, "-f", &format!("| herdr_field {keys}"))
            .ok_or(Error::UsageMixedSecrets)?;
        self.variables += 1;
        let variable = format!("herdr_s{}", self.variables);
        let output = self.run(
            &format!("{variable}=$({curl}) && [ -n \"${variable}\" ]"),
            request.timeout + Duration::from_secs(5),
        )?;
        if output.success {
            Ok(Secret(Held::There(variable)))
        } else {
            Err(Error::UsageRejected)
        }
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// Text for a double-quoted curl config value, inside an unquoted
/// here-document: curl's escapes first, then the shell's.
fn escape(text: &str) -> String {
    text.replace('\\', "\\\\\\\\")
        .replace('"', "\\\\\"")
        .replace('$', "\\$")
        .replace('`', "\\`")
        .replace('\n', "\\\\n")
}

fn splice(parts: &[Part]) -> Option<String> {
    let mut text = String::new();
    for part in parts {
        match part {
            Part::Text(value) => text.push_str(&escape(value)),
            Part::Secret(Secret(Held::There(variable))) => {
                text.push_str(&format!("${{{variable}}}"));
            }
            // This machine's secrets never go to the host.
            Part::Secret(Secret(Held::Here(_))) => return None,
        }
    }
    Some(text)
}

/// Runs `command` to completion with a deadline, keeping at most `LIMIT`
/// bytes of its standard output. Standard error is discarded: it may echo
/// what the child was reading.
pub(super) fn output(
    command: &mut Command,
    timeout: Duration,
    operation: &'static str,
) -> Result<(bool, Zeroizing<Vec<u8>>)> {
    let process = |source| Error::UsageProcess { operation, source };
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(process)?;
    let Some(stdout) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return Err(process(std::io::Error::other("no output pipe")));
    };
    let (sender, reads) = mpsc::sync_channel(1);
    let reader = thread::Builder::new()
        .name("herdr-usage-output".into())
        .spawn(move || {
            let mut bytes = Zeroizing::new(Vec::new());
            let result = stdout
                .take(LIMIT as u64 + 1)
                .read_to_end(&mut bytes)
                .map(|_| bytes);
            let _ = sender.send(result);
        });
    if let Err(source) = reader {
        let _ = child.kill();
        let _ = child.wait();
        return Err(process(source));
    }
    let result = match reads.recv_timeout(timeout) {
        Ok(Ok(bytes)) if bytes.len() > LIMIT => Err(Error::UsageSize),
        Ok(Ok(bytes)) => Ok(bytes),
        Ok(Err(source)) => Err(process(source)),
        Err(_) => Err(Error::UsageTimeout),
    };
    let bytes = match result {
        Ok(bytes) => bytes,
        Err(error) => {
            // Killing the child closes the pipe, which ends the reader.
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
    };
    let status = child.wait().map_err(process)?;
    Ok((status.success(), bytes))
}
