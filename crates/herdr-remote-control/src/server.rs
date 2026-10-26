//! `herdr-remote-control serve`: loads the state directory (certificate authority,
//! devices, VAPID key), connects to the local Herdr daemon, and serves the
//! web app over HTTPS until Herdr UI asks it to stop.

mod control;
mod listener;
mod live;
mod notifier;
mod options;

pub use crate::push::Privacy;
pub use options::{Certificate, Invocation, Options, ParseError, USAGE, parse};

use crate::{
    Result,
    auth::{Auth, Limits},
    hub::Hub,
    push::{Sender, Vapid},
    tls::{self, Authority, Names, Resolver},
    web::{self, State},
};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    net::IpAddr,
    sync::{Arc, Mutex, RwLock},
    time::{Duration, SystemTime},
};
use tokio::sync::watch;

/// How often the served addresses are checked, so a new network or
/// Tailscale coming up gets a certificate that names it.
const ADDRESS_CHECK: Duration = Duration::from_secs(30);
/// How often idle devices and expired pictures are forgotten.
const IDLE_CHECK: Duration = Duration::from_secs(60 * 60);
/// Who push services may contact about this sender.
const PUSH_SUBJECT: &str = "https://github.com/penso/herdr-gpui";
/// How long shutdown waits for open connections, such as phones' live
/// streams, before the daemon exits anyway.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(2);

/// The control endpoint's answer, for other modules' tests.
#[cfg(test)]
pub(crate) fn answer_for_tests(
    state: &State,
    stop: &watch::Sender<bool>,
    request: crate::control::Request,
) -> crate::control::Response {
    control::answer(state, stop, request)
}

/// Every device's APNs registration, for other modules' tests.
#[cfg(test)]
pub(crate) fn registrations_for_tests(
    state: &State,
) -> Vec<(crate::control::DeviceId, crate::apns::Registration)> {
    live::registrations(state)
}

/// Runs the daemon on the current Tokio runtime until it is told to stop.
pub async fn run(options: Options) -> Result<()> {
    std::fs::create_dir_all(&options.state_dir)?;
    let bind = options.bind;
    let (authority, resolver, names) = match &options.certificate {
        Certificate::Own => {
            let names = Names::current().bound_to(bind);
            let authority = Authority::load_or_create(&options.state_dir)?;
            let resolver = Resolver::issue(&authority, names.clone(), SystemTime::now())?;
            (Some(authority), resolver, names)
        }
        Certificate::Files { cert, key } => {
            let resolver = Resolver::from_files(cert, key)?;
            // Phones reach a user's certificate by the names it lists.
            let names = Names::from_certificate(resolver.current().cert[0].as_ref()).bound_to(bind);
            (None, resolver, names)
        }
    };
    let fingerprint = match &authority {
        Some(authority) => authority.fingerprint().to_owned(),
        None => colon_hex(&Sha256::digest(resolver.current().cert[0].as_ref())),
    };
    let resolver = Arc::new(resolver);
    let config = Arc::new(tls::server_config(resolver.clone())?);
    let push = Vapid::load_or_create(&options.state_dir)
        .and_then(|vapid| Sender::new(vapid, PUSH_SUBJECT))
        .ok();
    let auth = Auth::load(&options.state_dir, Limits::default())?;
    let apps = auth.apps();
    let apns = crate::apns::Sender::load(&options.state_dir)?;
    // Only origins this computer's own authority serves may be remembered.
    let seen = match &authority {
        Some(authority) => web::Origins::load(&options.state_dir, authority.fingerprint()),
        None => web::Origins::none(),
    };
    let hub = Hub::start(options.herdr_socket.clone(), options.chat_dir.clone())?;
    let state = Arc::new(State {
        apns,
        apps,
        auth: Mutex::new(auth),
        authority,
        computer: options::computer_name(),
        connected: Mutex::new(HashMap::new()),
        fingerprint,
        hub,
        look: web::Look::load(&options.state_dir),
        names: RwLock::new(names),
        port: options.port,
        privacy: options.privacy,
        push,
        reports: Mutex::new(web::report_limit()),
        seen: Mutex::new(seen),
        uploads: web::Uploads::new(&options.state_dir),
    });
    remember_origins(&state);
    // The port first: a second daemon fails here before it touches the
    // first one's control socket.
    let tcp = listener::bind(options.bind, options.port)?;
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        port = options.port,
        "Remote control started"
    );
    let (stop, stopped) = watch::channel(false);
    control::serve(&options.state_dir, state.clone(), stop.clone())?;
    // Only once this daemon owns the port and the control socket: a second
    // copy started on the same state must not delete the first one's
    // pictures.
    state.uploads.remove_leftovers();
    tokio::spawn(notifier::run(state.clone()));
    // A user's certificate lists fixed names; only the daemon's own follows
    // the computer's addresses.
    if state.authority.is_some() {
        tokio::spawn(watch_addresses(state.clone(), resolver, bind));
    }
    tokio::spawn(expire(state.clone(), options.device_idle));
    let app = web::router(state).into_make_service_with_connect_info::<web::Peer>();
    let mut shutdown = stopped.clone();
    let serve = axum::serve(listener::TlsListener::new(tcp, config), app)
        .with_graceful_shutdown(async move {
            let _ = shutdown.wait_for(|stop| *stop).await;
        })
        .into_future();
    until_stopped(serve, stopped, SHUTDOWN_GRACE).await?;
    crate::control::release(&options.state_dir);
    Ok(())
}

/// Runs `serve` until it ends, or until `grace` after a stop. A phone's
/// live stream never ends on its own, so graceful shutdown waits only so
/// long for open connections before the daemon exits anyway.
async fn until_stopped(
    serve: impl Future<Output = std::io::Result<()>>,
    mut stopped: watch::Receiver<bool>,
    grace: Duration,
) -> std::io::Result<()> {
    tokio::select! {
        served = serve => served,
        () = async move {
            let _ = stopped.wait_for(|stop| *stop).await;
            tokio::time::sleep(grace).await;
        } => Ok(()),
    }
}

/// Reissues the certificate when the computer's addresses change.
async fn watch_addresses(state: Arc<State>, resolver: Arc<Resolver>, bind: IpAddr) {
    let mut ticker = tokio::time::interval(ADDRESS_CHECK);
    loop {
        ticker.tick().await;
        let Ok(names) = tokio::task::spawn_blocking(move || Names::current().bound_to(bind)).await
        else {
            continue;
        };
        if let Some(authority) = &state.authority
            && resolver
                .refresh(authority, &names, SystemTime::now())
                .is_err()
        {
            continue;
        }
        if let Ok(mut current) = state.names.write() {
            *current = names;
        }
        let state = state.clone();
        let _ = tokio::task::spawn_blocking(move || remember_origins(&state)).await;
    }
}

/// Adds the origins served now to the ones an installed app may still
/// send. Blocking; a failure to save only forgets them at the next start.
fn remember_origins(state: &State) {
    let origins = state.origins();
    let _ = state
        .seen
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .remember(&origins);
}

async fn expire(state: Arc<State>, idle: Option<Duration>) {
    let idle_ms = idle.map(|idle| u64::try_from(idle.as_millis()).unwrap_or(u64::MAX));
    let mut ticker = tokio::time::interval(IDLE_CHECK);
    loop {
        ticker.tick().await;
        let state = state.clone();
        let _ = tokio::task::spawn_blocking(move || sweep(&state, web::now_ms(), idle_ms)).await;
    }
}

/// One tick of [`expire`]: drops old pictures, and devices away for
/// `idle_ms`; with no limit, devices stay until revoked. Blocking.
fn sweep(state: &State, now_ms: u64, idle_ms: Option<u64>) {
    // Pictures expire on their own schedule, whether or not another
    // upload comes to sweep them.
    state.uploads.prune(now_ms);
    if let Some(idle_ms) = idle_ms {
        let _ = state.auth().expire_idle(now_ms, idle_ms);
    }
}

/// Uppercase colon-separated hex, as certificate fingerprints read.
fn colon_hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(":")
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
