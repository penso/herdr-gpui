// Herdr Remote: the app shell's script. It routes between server-rendered
// fragments, keeps a signed session with the computer, applies the
// server's patch stream, and sends commands. No inline script and no
// eval: the page's CSP forbids both, and Alpine runs as its CSP build.
"use strict";

// Every computer this app paired with, the one shown first. A computer is
// known by its certificate's fingerprint; its device key is kept under
// `device:ID` in IndexedDB.
const HOSTS = "herdr.hosts"; // [{ id, origins, fingerprint, computer }]
/// The one computer paired before this app held several; `migrate` moves it.
const DEVICE = "herdr.device";
/// The most computers, and addresses per computer, a phone keeps.
const MAX_HOSTS = 16;
const MAX_ORIGINS = 16;
/// Which computer this app's notifications come from: a push subscription
/// carries one computer's sender key.
const PUSH_HOST = "herdr.push-host";
const TOKEN = "herdr.session"; // { token, expires, origin, device }
const SIGN_PREFIX = "herdr-remote-session\0";
const INSTALL_CLOSED = "herdr.install-closed"; // true once the footer is closed
/// How long the row a list was left for stays marked; `.came-from` in input.css.
const MARK_MS = 1600;
/// How many commands the composer lists under a typed "/".
const SLASH_SHOWN = 5;
const HANDOFF = "herdr.handoff"; // { token, expires_ms } a tab offers its Home Screen app
const RELOADED = "herdr.pair-reloaded"; // set once a failed pairing reloaded without the cache

const app = {
  // The computer shown, `hosts()[0]`.
  device: null,
  session: readJson(sessionStorage, TOKEN),
  origin: null,
  stream: null,
  // The navigation in progress; a newer one aborts it.
  routing: null,
  // The path shown last, and the chat or terminal the list was left for:
  // coming back to the list scrolls to that row and marks it.
  path: null,
  cameFrom: null,
  // The row being marked and when the mark started, so a patch that
  // replaces the list marks the new row where the fade had reached.
  mark: null,
  retry: 0,
  // Android's install prompt, kept until the footer's Install uses it.
  install: null,
  // Counts the views shown, so work begun in one leaves alone a view the
  // phone has moved on to.
  views: 0,
};

function readJson(storage, key) {
  try {
    return JSON.parse(storage.getItem(key) || "null");
  } catch {
    return null;
  }
}

function writeJson(storage, key, value) {
  try {
    if (value === null) storage.removeItem(key);
    else storage.setItem(key, JSON.stringify(value));
  } catch {
    // Private browsing: the session lives in memory only.
  }
}

// ---- the paired computers ----

function hosts() {
  const list = readJson(localStorage, HOSTS);
  if (Array.isArray(list)) return list.filter((host) => host?.id && host.fingerprint);
  // Not moved yet (`migrate`): the one computer an older version paired.
  const legacy = readJson(localStorage, DEVICE);
  return legacy?.id && legacy.fingerprint ? [legacy] : [];
}

function saveHosts(list) {
  writeJson(localStorage, HOSTS, list.slice(0, MAX_HOSTS));
  app.device = list[0] || null;
}

/// Shows `host` from now on: it moves first, replacing an older pairing
/// with the same computer.
function useHost(host) {
  // Keys and pastes still queued were typed into the other computer's panes.
  if (app.device && app.device.fingerprint !== host.fingerprint) term.queue = [];
  saveHosts([host, ...hosts().filter((other) => other.fingerprint !== host.fingerprint)]);
  if (app.session?.device !== host.id) app.session = null;
}

/// Saves what changed about `host` without moving it.
function keepHost(host) {
  const list = hosts();
  const index = list.findIndex((other) => other.fingerprint === host.fingerprint);
  if (index < 0) return;
  list[index] = host;
  saveHosts(list);
}

/// Moves the one computer an older version paired, and its key, into the
/// list. Nothing old is deleted until the new copies are saved: a storage
/// failure leaves the pairing where it was (`hosts` and `loadKey` read it
/// there) and the next launch tries again.
async function migrate() {
  const legacy = readJson(localStorage, DEVICE);
  if (!legacy) return;
  try {
    if (legacy.id && legacy.fingerprint) {
      const pair = await keyStore("readonly", (store) => store.get("device"));
      if (pair) await saveKey(legacy, pair);
      const list = readJson(localStorage, HOSTS);
      const known = Array.isArray(list) ? list : [];
      if (!known.some((host) => host.fingerprint === legacy.fingerprint)) {
        writeJson(localStorage, HOSTS, [...known, legacy].slice(0, MAX_HOSTS));
      }
      const saved = readJson(localStorage, HOSTS);
      if (!Array.isArray(saved) || !saved.some((host) => host.fingerprint === legacy.fingerprint)) return;
      // Any push subscription was that computer's, the only one there was.
      if (!readJson(localStorage, PUSH_HOST)) writeJson(localStorage, PUSH_HOST, legacy.fingerprint);
    }
    writeJson(localStorage, DEVICE, null);
    await keyStore("readwrite", (store) => store.delete("device"));
  } catch {
    // Storage is unavailable now; the next launch tries again.
  }
}

// ---- bytes ----

function b64url(bytes) {
  let text = "";
  for (const byte of new Uint8Array(bytes)) text += String.fromCharCode(byte);
  return btoa(text).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

function fromB64url(text) {
  const padded = text.replace(/-/g, "+").replace(/_/g, "/").padEnd(Math.ceil(text.length / 4) * 4, "=");
  return Uint8Array.from(atob(padded), (char) => char.charCodeAt(0));
}

// ---- the device key, kept in IndexedDB and never extractable ----

function keyStore(mode, action) {
  return new Promise((resolve, reject) => {
    const open = indexedDB.open("herdr-remote-control", 1);
    open.onupgradeneeded = () => open.result.createObjectStore("keys");
    open.onerror = () => reject(open.error);
    open.onsuccess = () => {
      const tx = open.result.transaction("keys", mode);
      const request = action(tx.objectStore("keys"));
      request.onsuccess = () => resolve(request.result);
      request.onerror = () => reject(request.error);
    };
  });
}

const keyName = (host) => `device:${host.id}`;
/// `host`'s key, or the one an older version kept for its only computer
/// while `migrate` has not moved it.
async function loadKey(host) {
  const key = await keyStore("readonly", (store) => store.get(keyName(host)));
  if (key || readJson(localStorage, DEVICE)?.id !== host.id) return key;
  return keyStore("readonly", (store) => store.get("device"));
}
const saveKey = (host, pair) => keyStore("readwrite", (store) => store.put(pair, keyName(host)));
const dropKey = (host) => keyStore("readwrite", (store) => store.delete(keyName(host)));

// ---- talking to the computer ----

class HttpError extends Error {
  constructor(status, message) {
    super(message);
    this.status = status;
  }
}

async function call(path, { method = "GET", body, raw, auth = true, origin, signal } = {}) {
  const headers = {};
  if (body !== undefined) headers["Content-Type"] = "application/json";
  if (raw !== undefined) headers["Content-Type"] = raw.type || "application/octet-stream";
  if (auth && app.session) headers.Authorization = `Bearer ${app.session.token}`;
  const response = await fetch((origin || app.origin || "") + path, {
    method,
    headers,
    body: raw !== undefined ? raw : body === undefined ? undefined : JSON.stringify(body),
    cache: "no-store",
    signal,
  });
  if (!response.ok) {
    let message = response.statusText;
    try {
      message = (await response.json()).error || message;
    } catch {
      // Not JSON.
    }
    throw new HttpError(response.status, message);
  }
  return response;
}

async function api(path, options = {}) {
  try {
    return await call(path, options);
  } catch (error) {
    if (error.status !== 401 || options.auth === false) throw error;
    app.session = null;
    await signIn();
    return call(path, options);
  }
}

/// Writes an error this screen hit to the computer's log (`daemon.log`),
/// where it can be read; best effort, and never about chat content.
function report(context, error) {
  const where = `${installed() ? "Home Screen app" : "browser"} at ${location.pathname}`;
  const message = `${error?.name || "Error"}: ${error?.message || error} (${where})`;
  fetch(`${app.origin || ""}/api/log`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ context, message }),
    keepalive: true,
  }).catch(() => {});
}

window.addEventListener("error", (event) => report("script", event.error || event.message));
window.addEventListener("unhandledrejection", (event) => report("script", event.reason));

/// The first of `host`'s addresses that answers as that computer, this
/// page's first. An address may answer as another computer, after a
/// network change or on another network, so the fingerprint must match.
async function reach(host) {
  const origins = [location.origin, ...(host.origins || [])].filter(
    (origin, index, all) => all.indexOf(origin) === index,
  );
  const tries = origins.map(async (origin) => {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), 2500);
    try {
      const response = await fetch(`${origin}/api/ping`, { signal: controller.signal, cache: "no-store" });
      if (!response.ok) throw new Error("not ok");
      if ((await response.json()).fingerprint !== host.fingerprint) throw new Error("another computer");
      return origin;
    } finally {
      clearTimeout(timer);
    }
  });
  return Promise.any(tries);
}

/// The computer to show and the address it answers at: the one shown last
/// when it answers, or else the first that does, in the order last shown.
async function pickHost() {
  const tries = hosts().map((host) => reach(host).then((origin) => ({ host, origin })));
  // Settled from the start, so a computer failing first is not reported
  // as an unhandled rejection.
  const others = Promise.allSettled(tries.slice(1));
  try {
    return await tries[0];
  } catch {
    // Wait for the others.
  }
  for (const result of await others) {
    if (result.status === "fulfilled") return result.value;
  }
  throw new Error("No paired computer answers.");
}

async function signIn() {
  if (
    app.session &&
    app.session.expires > Date.now() + 60_000 &&
    app.session.origin === app.origin &&
    app.session.device === app.device?.id
  ) {
    return;
  }
  const pair = app.device && (await loadKey(app.device));
  if (!pair) throw new HttpError(403, "This phone is not paired.");
  const challenge = await (
    await call("/api/session/challenge", { method: "POST", body: { device: app.device.id }, auth: false })
  ).json();
  const message = new Uint8Array([...new TextEncoder().encode(SIGN_PREFIX), ...fromB64url(challenge.nonce)]);
  const signature = await crypto.subtle.sign({ name: "ECDSA", hash: "SHA-256" }, pair.privateKey, message);
  const session = await (
    await call("/api/session", {
      method: "POST",
      body: { challenge: challenge.challenge, signature: b64url(signature) },
      auth: false,
    })
  ).json();
  app.session = { token: session.token, expires: session.expires_ms, origin: app.origin, device: app.device.id };
  writeJson(sessionStorage, TOKEN, app.session);
  rememberOrigins(session.origins);
}

/// Saves the addresses served now on every connect, not only at sign-in: a
/// session outlives a network change, and the installed app must still find
/// the computer on its next launch.
async function refreshOrigins() {
  try {
    rememberOrigins((await (await api("/api/origins")).json()).origins);
  } catch {
    // The next connect tries again.
  }
}

/// Keeps the addresses the computer serves now first, then the ones known
/// before, which may work again on another network.
function rememberOrigins(current) {
  if (!Array.isArray(current) || !app.device) return;
  const known = [...current, ...(app.device.origins || [])].filter(
    (origin, index, all) => typeof origin === "string" && origin.startsWith("https://") && all.indexOf(origin) === index,
  );
  keepHost({ ...app.device, origins: known.slice(0, MAX_ORIGINS) });
}

// ---- views ----

const view = () => document.getElementById("view");

/// One of this app's own templates, which sit directly in the body. A
/// computer's fragment, inside the view, may carry an element with the same
/// id; looking up by id alone would find that one first.
const template = (id) => document.querySelector(`body > template#${id}`);

function show(id) {
  stopStream();
  stopScan();
  app.views++;
  view().replaceChildren(template(id).content.cloneNode(true));
  view().dataset.local = "";
}

/// Whether `element` sits in a view this app drew from its own templates,
/// not in a fragment a computer sent. Any paired computer's markup runs in
/// this page, so controls over the paired computers answer only here.
function local(element) {
  return "local" in view().dataset && view().contains(element);
}

function banner(text) {
  const element = document.getElementById("banner");
  element.textContent = text || "";
  element.hidden = !text;
}

function flash(text) {
  banner(text);
  setTimeout(() => banner(""), 4000);
}

/// The fragment and stream a path shows.
function routeOf(path, search) {
  const chat = path.match(/^\/chat\/([^/]+)$/);
  if (chat) return { fragment: `/f/chat/${chat[1]}`, stream: `view=chat&pane=${chat[1]}` };
  const term = path.match(/^\/term\/([^/]+)$/);
  if (term) return { fragment: `/f/term/${term[1]}`, stream: `view=term&pane=${term[1]}` };
  const routes = {
    "/": { fragment: "/f/home", stream: "view=home" },
    "/board": { fragment: "/f/board", stream: "view=board" },
    "/new": { fragment: "/f/new" },
    "/new-chat": { fragment: `/f/new-chat${search}` },
    "/settings": { fragment: "/f/settings" },
  };
  return routes[path] || routes["/"];
}

/// How deep a path sits in the navigation stack: the lists are the root,
/// and everything opened from them one level in.
function depth(path) {
  return path === "/" || path === "/board" ? 0 : 1;
}

/// Swaps the view with `update`, sliding like an iOS navigation stack: in
/// from the right when going deeper, back to the left when leaving.
/// Safari's own swipe-back gesture already animates, so it is not doubled.
function slide(from, to, event, update) {
  const direction = depth(to) > depth(from) ? "push" : depth(to) < depth(from) ? "pop" : null;
  const still = matchMedia("(prefers-reduced-motion: reduce)").matches;
  if (!from || !direction || still || event?.hasUAVisualTransition || !document.startViewTransition) {
    return update();
  }
  document.documentElement.dataset.slide = direction;
  const transition = document.startViewTransition(update);
  transition.finished.finally(() => delete document.documentElement.dataset.slide);
  return transition.updateCallbackDone;
}

async function route(event) {
  const target = routeOf(location.pathname, location.search);
  const from = app.path;
  app.mark = null;
  if (/^\/(chat|term)\//.test(from || "")) app.cameFrom = from;
  app.path = location.pathname;
  delete document.documentElement.dataset.sheet;
  stopStream();
  app.views++;
  // Only the newest navigation may fill the view or open a stream: an
  // older page answering late must not replace the one the URL names.
  app.routing?.abort();
  const routing = new AbortController();
  app.routing = routing;
  // The computers list is this phone's own, drawn here and shown even when
  // no computer answers.
  if (location.pathname === "/computers") return slide(from, location.pathname, event, showComputers);
  // Left for a list with no computer connected, such as from the computers
  // list while none answered: connect first, never to this page's own.
  if (!app.origin) return start(true);
  try {
    const html = await (await api(target.fragment, { signal: routing.signal })).text();
    if (app.routing !== routing) return;
    await slide(from, location.pathname, event, async () => {
      delete view().dataset.local;
      view().innerHTML = html;
      for (const link of view().querySelectorAll("a[data-computer]")) {
        link.href = app.origin + link.getAttribute("href");
      }
      // Alpine sets up the new fragment from a mutation observer; let it
      // show or hide groups before measuring rows.
      await Promise.resolve();
      applySearch();
      window.scrollTo(0, location.pathname.startsWith("/chat/") ? document.body.scrollHeight : 0);
      revealCameFrom();
    });
    fitTerminal();
    showInstallHint();
    offerHandoff();
    if (target.stream) startStream(target.stream);
  } catch (error) {
    if (app.routing !== routing || routing.signal.aborted) return;
    if (error.status === 404) return navigate("/");
    if (error.status === 403) return forget();
    flash(error.message);
  }
}

/// Scrolls the list to the row of the chat or terminal just left, and
/// marks it briefly.
function revealCameFrom() {
  const path = app.cameFrom;
  app.cameFrom = null;
  if (!path || /^\/(chat|term)\//.test(location.pathname)) return;
  app.mark = { path, at: performance.now() };
  markRow()?.scrollIntoView({ block: "center" });
}

/// Marks the row `app.mark` names, resuming its fade where it was, and
/// returns it. The list's own row wins over a chat card in the strip.
function markRow() {
  if (!app.mark) return null;
  const elapsed = performance.now() - app.mark.at;
  if (elapsed > MARK_MS) {
    app.mark = null;
    return null;
  }
  const links = [...view().querySelectorAll("a[data-nav]")].filter(
    (link) => link.pathname === app.mark.path && link.getClientRects().length > 0,
  );
  const row = links.find((link) => link.classList.contains("row")) || links[0];
  if (row && !row.classList.contains("came-from")) {
    row.style.animationDelay = `-${Math.round(elapsed)}ms`;
    row.classList.add("came-from");
  }
  return row;
}

function navigate(path) {
  if (path !== location.pathname + location.search) history.pushState(null, "", path);
  route();
}

// ---- the patch stream: server-sent events read with fetch, so the
// bearer token rides in a header rather than the URL ----

function stopStream() {
  app.stream?.abort();
  app.stream = null;
}

async function startStream(query) {
  const controller = new AbortController();
  app.stream = controller;
  try {
    const response = await call(`/api/stream?${query}`, { signal: controller.signal });
    if (app.stream !== controller) return response.body?.cancel();
    app.retry = 0;
    banner("");
    const reader = response.body.pipeThrough(new TextDecoderStream()).getReader();
    let buffer = "";
    controller.signal.addEventListener("abort", () => reader.cancel());
    for (;;) {
      const { value, done } = await reader.read();
      if (done) break;
      buffer += value;
      let end;
      while ((end = buffer.indexOf("\n\n")) >= 0) {
        handleEvent(buffer.slice(0, end));
        buffer = buffer.slice(end + 2);
      }
    }
  } catch (error) {
    if (controller.signal.aborted) return;
    if (error.status === 401) app.session = null;
  }
  if (controller.signal.aborted || app.stream !== controller) return;
  // The stream ended: reconnect, reloading the view to catch up.
  banner("Reconnecting…");
  app.retry = Math.min(app.retry + 1, 6);
  setTimeout(() => app.stream === controller && start(), 500 * 2 ** app.retry);
}

function handleEvent(block) {
  let event = "message";
  let data = "";
  for (const line of block.split("\n")) {
    if (line.startsWith("event:")) event = line.slice(6).trim();
    else if (line.startsWith("data:")) data += line.slice(5).trim();
  }
  if (event === "reload") return route();
  if (event !== "patch" || !data) return;
  const patch = JSON.parse(data);
  if (patch.badge !== undefined) navigator.setAppBadge?.(patch.badge).catch(() => {});
  // A chat read at its end keeps following what the agent writes.
  const atBottom = window.innerHeight + window.scrollY >= document.body.scrollHeight - 80;
  for (const [id, html] of Object.entries(patch.regions || {})) {
    const element = document.getElementById(id);
    // A computer patches only what it drew: never this app's own chrome.
    if (!element || element === view() || !view().contains(element) || "local" in view().dataset) continue;
    // Never replace a composer holding a draft, pictures, or a send.
    const draft = element.querySelector?.("textarea");
    const busy = element.matches?.("[data-busy]") || element.querySelector?.("[data-busy]");
    if ((draft && (draft.value || document.activeElement === draft)) || busy) {
      syncComposer(element, html);
      continue;
    }
    element.outerHTML = html;
    if (id === "term-screen") fitTerminal();
  }
  // New chat entries: the server sends only what changed after the
  // stream's first patch, so these follow the entries already shown.
  for (const [id, html] of Object.entries(patch.append || {})) {
    const element = document.getElementById(id);
    if (!element || !view().contains(element) || "local" in view().dataset) continue;
    element.insertAdjacentHTML("beforeend", html);
  }
  if (atBottom && document.getElementById("chat-turn")) window.scrollTo(0, document.body.scrollHeight);
  applySearch();
  markRow();
}

/// Hides the rows of home or the board that lack a searched word, and
/// the groups left empty. Runs again after each update, which replaces the
/// lists but not the search field.
function applySearch() {
  const input = document.getElementById("search");
  if (!input) return;
  const words = input.value.toLowerCase().split(/\s+/).filter(Boolean);
  // A folded workspace opens while searching, so its matches show.
  view().classList.toggle("searching", words.length > 0);
  let shown = 0;
  for (const row of view().querySelectorAll("[data-find]")) {
    const text = row.dataset.find.toLowerCase();
    row.hidden = !words.every((word) => text.includes(word));
    if (!row.hidden) shown += 1;
  }
  for (const group of view().querySelectorAll("[data-group]")) {
    group.hidden = words.length > 0 && !group.querySelector("[data-find]:not([hidden])");
  }
  for (const strip of view().querySelectorAll("[data-strip]")) strip.hidden = words.length > 0;
  const none = document.getElementById("no-match");
  if (none) none.hidden = words.length === 0 || shown > 0;
}

/// The Add to Home Screen footer, on home and the board of a browser tab
/// until it is closed: notifications on iPhone need the installed app.
/// Whether this page runs as the Home Screen app rather than a browser tab.
function installed() {
  return matchMedia("(display-mode: standalone)").matches || navigator.standalone === true;
}

function showInstallHint() {
  const hint = document.getElementById("install-hint");
  if (!hint) return;
  const apple = /iPhone|iPad/.test(navigator.userAgent) || (/Macintosh/.test(navigator.userAgent) && navigator.maxTouchPoints > 1);
  const lists = location.pathname === "/" || location.pathname === "/board";
  hint.hidden = installed() || !lists || !app.device || readJson(localStorage, INSTALL_CLOSED) === true || !(apple || app.install);
  hint.querySelector("[data-install]").hidden = !app.install;
  hint.querySelector("[data-install-how]").hidden = Boolean(app.install);
}

/// Lets the Home Screen app take over this tab's pairing, as iOS gives it
/// storage of its own. Its start address carries a single-use token as
/// `?handoff=`: Safari reads the manifest when the page loads, so the page
/// reloads once with the token, which the daemon then puts in the manifest
/// link too. One token serves until it nears expiry, so an icon already
/// added keeps a valid one.
async function offerHandoff() {
  if (installed() || !app.device) return;
  if (location.pathname !== "/" && location.pathname !== "/board") return;
  // The Home Screen app redeems the token at its own origin, so only the
  // computer this page was loaded from may offer one.
  if (!app.device.origins?.includes(location.origin)) return;
  let offer = readJson(localStorage, HANDOFF);
  if (!offer?.token || offer.expires_ms - Date.now() < 3_600_000) {
    try {
      offer = await (await api("/api/handoff", { method: "POST" })).json();
    } catch {
      return;
    }
    writeJson(localStorage, HANDOFF, offer);
  }
  const address = `${location.pathname}?handoff=${encodeURIComponent(offer.token)}`;
  const link = document.querySelector('link[rel="manifest"]');
  const loaded = link && new URL(link.href).searchParams.get("handoff");
  if (loaded === offer.token) {
    history.replaceState(history.state, "", address);
  } else if (new URLSearchParams(location.search).get("handoff") !== offer.token) {
    location.replace(address);
  }
}

/// The Home Screen app's first launch: takes over the browser's pairing
/// with a key of its own, asking nothing.
async function redeemHandoff(token) {
  show("t-loading");
  history.replaceState(null, "", "/");
  try {
    const pairKey = await crypto.subtle.generateKey({ name: "ECDSA", namedCurve: "P-256" }, false, ["sign"]);
    const publicKey = await crypto.subtle.exportKey("raw", pairKey.publicKey);
    const answer = await (
      await call("/api/handoff/redeem", { method: "POST", auth: false, body: { token, public_key: b64url(publicKey) } })
    ).json();
    // Redeemed on the page of the computer that made the handoff, so its
    // own answer names it.
    await adopt(pairKey, answer, answer.fingerprint);
    return start();
  } catch (failure) {
    report("handoff", failure);
    show("t-unpaired");
    view().querySelector("[data-reason]").textContent =
      `${failure.message}. Open Herdr in the browser, then add it to the Home Screen again.`;
  }
}

/// Keeps the key and device an approval or a handoff granted, and shows
/// that computer. Pairing a computer again replaces its older pairing.
async function adopt(pairKey, answer, fingerprint) {
  // The code scanned off the desktop names the computer; one answering with
  // another fingerprint must not take, or replace, a pairing as that one.
  // The code carries it bare; the computer reports it colon-separated.
  const bare = (text) => String(text || "").replace(/:/g, "").toUpperCase();
  if (!fingerprint || bare(answer.fingerprint) !== bare(fingerprint)) {
    throw new Error("This code belongs to another computer.");
  }
  const host = { id: answer.device, origins: answer.origins, fingerprint: answer.fingerprint, computer: answer.computer };
  const older = hosts().find((other) => other.fingerprint === host.fingerprint && other.id !== host.id);
  await saveKey(host, pairKey);
  if (older) await dropKey(older).catch(() => {});
  useHost(host);
  app.origin = null;
  writeJson(sessionStorage, TOKEN, null);
}

window.addEventListener("beforeinstallprompt", (event) => {
  event.preventDefault();
  app.install = event;
  showInstallHint();
});

// ---- commands and forms ----

/// Sends a command; true when the chat took it.
async function command(pane, body) {
  try {
    const answer = await (await api(`/api/chat/${pane}/command`, { method: "POST", body })).json();
    if (!answer.rejected) return true;
    flash(answer.rejected);
  } catch (error) {
    flash(error.message);
  }
  return false;
}

document.addEventListener("click", async (event) => {
  if (event.target.closest("[data-install-close]")) {
    writeJson(localStorage, INSTALL_CLOSED, true);
    return showInstallHint();
  }
  if (event.target.closest("[data-install]") && app.install) {
    const prompt = app.install;
    app.install = null;
    await prompt.prompt();
    return showInstallHint();
  }
  // The session sheet: kept open on <html> so a patch redrawing it does
  // not close it.
  if (event.target.closest("[data-open-settings]")) {
    document.documentElement.dataset.sheet = "settings";
    return;
  }
  if (event.target.closest("[data-close-settings]")) {
    delete document.documentElement.dataset.sheet;
    return;
  }
  const link = event.target.closest("a[data-nav]");
  if (link && link.origin === location.origin) {
    event.preventDefault();
    return navigate(link.pathname + link.search);
  }
  const button = event.target.closest("[data-command]");
  if (button) {
    event.preventDefault();
    button.disabled = true;
    command(button.dataset.pane, JSON.parse(button.dataset.command)).finally(() => (button.disabled = false));
  }
  const own = (selector) => {
    const element = event.target.closest(selector);
    return element && local(element) ? element : null;
  };
  if (own("[data-retry]")) start(true);
  const host = own("[data-host]");
  if (host) switchHost(host.dataset.host);
  if (own("[data-add-computer]")) startScan();
  if (own("[data-scan-cancel]")) navigate("/computers");
  terminalClick(event);
});

document.addEventListener("change", (event) => {
  const target = event.target;
  if (target.id === "term-file") {
    pastePicture(target.files[0]);
    target.value = "";
    return;
  }
  const pane = target.dataset.pane;
  if (target.dataset.option !== undefined) {
    command(pane, { command: "set_option", id: target.dataset.option, value: { type: "choice", id: target.value } });
  } else if (target.dataset.toggle !== undefined) {
    command(pane, { command: "set_option", id: target.dataset.toggle, value: { type: "toggle", value: target.checked } });
  } else if (target.dataset.mode !== undefined) {
    command(pane, { command: "set_mode", id: target.value });
  } else if (target.dataset.autoAccept !== undefined) {
    if (target.checked && !confirm("Answer every question with its allow option from now on?")) {
      target.checked = false;
      return;
    }
    command(pane, { command: "set_auto_accept", on: target.checked });
  }
});

document.addEventListener("submit", async (event) => {
  const form = event.target;
  if (!form.dataset.form) return;
  event.preventDefault();
  const body = Object.fromEntries(new FormData(form).entries());
  const error = form.querySelector(".form-error");
  // A new chat answers once it can be opened, which takes a moment; a
  // second tap must not start another one.
  if (form.dataset.sending !== undefined) return;
  form.dataset.sending = "";
  const submit = form.querySelector("[type=submit]");
  if (submit) submit.disabled = true;
  const shown = app.views;
  try {
    const answer = await (await api(form.dataset.form, { method: "POST", body })).json();
    // Left while it was sent, for another page or computer: stay there.
    if (app.views !== shown) return;
    // The form is spent: going back from what it made returns to the list.
    history.replaceState(null, "", answer.location || "/");
    route();
  } catch (failure) {
    if (error) error.textContent = failure.message;
  } finally {
    delete form.dataset.sending;
    if (submit) submit.disabled = false;
  }
});

window.addEventListener("popstate", route);

// ---- terminal tabs: the screen arrives as patches; what is typed goes to
// the pane key by key, in Herdr's key syntax, in order ----

const term = { ctrl: false, alt: false, queue: [], sending: false, zoomed: false };

const termPane = () => document.getElementById("term")?.dataset.pane;
const termInput = () => document.getElementById("term-input");

/// The key name Herdr reads for one typed character.
function keyOf(char) {
  if (char === " ") return "space";
  if (char === "+") return "plus";
  if (char === "\n" || char === "\r") return "enter";
  if (char === "\t") return "tab";
  return char;
}

/// `key` with the held modifiers, which a key then releases.
function modified(key) {
  let name = key;
  if (term.ctrl) name = `ctrl+${name.length === 1 ? name.toLowerCase() : name}`;
  if (term.alt) name = `alt+${name}`;
  if (term.ctrl || term.alt) {
    term.ctrl = term.alt = false;
    showModifiers();
  }
  return name;
}

function showModifiers() {
  for (const button of document.querySelectorAll("[data-mod]")) {
    button.classList.toggle("on", Boolean(term[button.dataset.mod]));
  }
}

/// Keys and pastes leave in the order typed, each to the pane shown when it
/// was typed, even when the page has moved on to another pane meanwhile.
function sendKeys(keys) {
  const pane = termPane();
  if (!pane || !keys.length) return;
  for (const key of keys) term.queue.push({ pane, key });
  flushInput();
}

function pasteText(text) {
  const pane = termPane();
  if (!pane || !text) return;
  term.queue.push({ pane, text });
  flushInput();
}

/// A picture goes in the same line as keys, so its path lands where it was
/// pasted; the computer stores it and types the path.
function pastePicture(file) {
  const pane = termPane();
  if (!pane || !file) return;
  term.queue.push({ pane, file });
  flushInput();
}

/// The Paste button: Safari asks to allow it, then hands over a screenshot
/// or text. A page cannot raise the native Paste menu over a hidden field.
async function pasteClipboard() {
  try {
    const items = (await navigator.clipboard?.read?.()) || [];
    for (const item of items) {
      const type = item.types.find((kind) => kind.startsWith("image/"));
      if (type) return pastePicture(await item.getType(type));
    }
    for (const item of items) {
      if (item.types.includes("text/plain")) return pasteText(await (await item.getType("text/plain")).text());
    }
    if (!navigator.clipboard?.read) return pasteText(await navigator.clipboard.readText());
    flash("The clipboard is empty.");
  } catch (error) {
    if (error.name !== "NotAllowedError") flash(error.message);
  }
}

/// The next request: one paste or picture, or up to 64 keys in a row for
/// one pane.
function nextInput() {
  const first = term.queue.shift();
  if (first.file !== undefined) return { pane: first.pane, kind: "upload", raw: first.file };
  if (first.text !== undefined) return { pane: first.pane, kind: "paste", body: { text: first.text } };
  const keys = [first.key];
  while (keys.length < 64 && term.queue.length && term.queue[0].pane === first.pane && term.queue[0].key !== undefined) {
    keys.push(term.queue.shift().key);
  }
  return { pane: first.pane, kind: "keys", body: { keys } };
}

async function flushInput() {
  if (term.sending) return;
  term.sending = true;
  while (term.queue.length) {
    const { pane, kind, body, raw } = nextInput();
    try {
      await api(`/api/term/${encodeURIComponent(pane)}/${kind}`, { method: "POST", body, raw });
    } catch (error) {
      // What followed was typed after it; sent alone, it would mean something else.
      term.queue = [];
      flash(error.message);
    }
  }
  term.sending = false;
}

/// Fits the widest line to the screen's width, unless the text was made
/// large; Herdr keeps the pane at the desktop's size.
function fitTerminal() {
  const screen = document.getElementById("term-screen");
  if (!screen) return;
  const page = document.getElementById("term");
  if (term.zoomed) {
    page.style.setProperty("--term-size", "13px");
  } else {
    const columns = Math.max(40, Number(screen.dataset.columns) || 0);
    const size = Math.min(13, Math.max(6, (screen.clientWidth - 16) / (columns * 0.61)));
    page.style.setProperty("--term-size", `${size.toFixed(2)}px`);
  }
  screen.scrollTop = screen.scrollHeight;
}

/// A composer holding a draft or focus is not replaced by a live patch,
/// but it still follows the agent's status from the patch it skipped, so
/// Stop turns back into Send when the agent finishes.
function syncComposer(element, html) {
  if (element.id !== "chat-composer") return;
  const fresh = new DOMParser().parseFromString(html, "text/html").getElementById("chat-composer");
  const text = element.querySelector("textarea");
  const freshText = fresh?.querySelector("textarea");
  if (!text || !freshText) return;
  Alpine.$data(element).working = "working" in fresh.dataset;
  text.placeholder = freshText.placeholder;
  text.disabled = freshText.disabled;
}

/// Keeps the terminal above the on-screen keyboard, and marks the page
/// while the keyboard is up so the composer drops the home-indicator inset.
function followViewport() {
  const viewport = window.visualViewport;
  if (!viewport) return;
  document.documentElement.style.setProperty("--vvh", `${viewport.height}px`);
  document.documentElement.toggleAttribute("data-keyboard", window.innerHeight - viewport.height > 120);
  const page = document.getElementById("term");
  if (page) page.style.transform = `translateY(${viewport.offsetTop}px)`;
  fitTerminal();
}
window.visualViewport?.addEventListener("resize", followViewport);
window.visualViewport?.addEventListener("scroll", followViewport);

function terminalClick(event) {
  if (!document.getElementById("term")) return;
  if (event.target.closest("#term-screen")) termInput()?.focus({ preventScroll: true });
  const key = event.target.closest("[data-key]");
  if (key) sendKeys([modified(key.dataset.key)]);
  const modifier = event.target.closest("[data-mod]");
  if (modifier) {
    term[modifier.dataset.mod] = !term[modifier.dataset.mod];
    showModifiers();
  }
  if (event.target.closest("[data-term-keyboard]")) {
    const input = termInput();
    if (document.activeElement === input) input.blur();
    else input?.focus({ preventScroll: true });
  }
  if (event.target.closest("[data-term-paste]")) pasteClipboard();
  if (event.target.closest("[data-term-picture]")) document.getElementById("term-file")?.click();
  if (event.target.closest("[data-term-zoom]")) {
    term.zoomed = !term.zoomed;
    fitTerminal();
  }
  const action = event.target.closest("[data-term-action]");
  if (action) termAction(action.dataset.termAction);
}

async function termAction(action) {
  const pane = termPane();
  if (!pane) return;
  if (action === "close_pane" && !confirm("Close this pane? Its program stops.")) return;
  if (action === "close_tab" && !confirm("Close this tab and every pane in it?")) return;
  try {
    const answer = await (
      await api(`/api/term/${encodeURIComponent(pane)}/action`, { method: "POST", body: { action } })
    ).json();
    navigate(answer.location || "/");
  } catch (error) {
    flash(error.message);
  }
}

// Keys above the keyboard keep the field focused, so the keyboard stays up.
document.addEventListener("pointerdown", (event) => {
  if (event.target.closest(".tkey, [data-term-paste]")) event.preventDefault();
});

document.addEventListener("input", (event) => {
  if (event.target.id === "search") applySearch();
});

document.addEventListener("beforeinput", (event) => {
  if (event.target.id !== "term-input") return;
  switch (event.inputType) {
    case "insertText":
    case "insertReplacementText":
      sendKeys([...(event.data || "")].map((char, index) => (index === 0 ? modified(keyOf(char)) : keyOf(char))));
      break;
    case "insertLineBreak":
    case "insertParagraph":
      sendKeys([modified("enter")]);
      break;
    case "deleteContentBackward":
      sendKeys([modified("backspace")]);
      break;
    case "insertFromPaste": {
      // A hardware keyboard's paste lands here; a screenshot comes as a file.
      const picture = [...(event.dataTransfer?.files || [])].find((file) => file.type.startsWith("image/"));
      if (picture) pastePicture(picture);
      else pasteText(event.dataTransfer?.getData("text/plain") || event.data || "");
      break;
    }
    case "insertCompositionText":
      // An input method is composing; its text comes at compositionend.
      return;
    default:
      break;
  }
  event.preventDefault();
});

document.addEventListener("compositionend", (event) => {
  if (event.target.id !== "term-input") return;
  sendKeys([...(event.data || "")].map(keyOf));
  event.target.value = "";
});

const HARDWARE_KEYS = {
  Enter: "enter",
  Backspace: "backspace",
  Tab: "tab",
  Escape: "esc",
  ArrowUp: "up",
  ArrowDown: "down",
  ArrowLeft: "left",
  ArrowRight: "right",
};

// A hardware keyboard: named keys and Ctrl or Alt chords. Plain characters
// arrive as input events, which input methods also use.
document.addEventListener("keydown", (event) => {
  if (event.target.id !== "term-input" || event.isComposing) return;
  const named = HARDWARE_KEYS[event.key];
  const chord = (event.ctrlKey || event.altKey) && event.key.length === 1;
  if (!named && !chord) return;
  event.preventDefault();
  let key = named || keyOf(event.key.toLowerCase());
  if (event.shiftKey && named === "tab") key = "shift+tab";
  if (event.altKey) key = `alt+${key}`;
  if (event.ctrlKey) key = `ctrl+${key}`;
  sendKeys([named && !chord ? modified(key) : key]);
});

// ---- computers: the list, switching, and adding one by its code ----

/// Lists the paired computers, each with the address it answers at now.
function showComputers() {
  show("t-computers");
  const list = view().querySelector("[data-hosts]");
  for (const host of hosts()) {
    const row = template("t-host").content.firstElementChild.cloneNode(true);
    row.dataset.host = host.fingerprint;
    row.querySelector("[data-name]").textContent = host.computer || "Computer";
    row.querySelector("[data-shown]").hidden = host.fingerprint !== app.device?.fingerprint;
    const address = row.querySelector("[data-address]");
    const dot = row.querySelector("[data-dot]");
    reach(host).then(
      (origin) => {
        address.textContent = new URL(origin).host;
        dot.classList.replace("bg-raised", "bg-green");
      },
      () => {
        address.textContent = "Not answering";
        dot.classList.replace("bg-raised", "bg-faint");
      },
    );
    list.append(row);
  }
}

/// Shows `fingerprint`'s computer from its home.
function switchHost(fingerprint) {
  const host = hosts().find((other) => other.fingerprint === fingerprint);
  if (!host) return;
  useHost(host);
  app.origin = null;
  history.pushState(null, "", "/");
  start();
}

// `attempt` counts scans started and stopped, so one cancelled while the
// camera starts stops the stream it gets.
const scan = { stream: null, timer: 0, attempt: 0 };

/// Loads the QR decoder the first time a code is scanned.
function loadDecoder() {
  if (window.jsQR) return Promise.resolve();
  return new Promise((resolve, reject) => {
    const script = document.createElement("script");
    script.src = "/assets/jsqr.min.js";
    script.onload = resolve;
    script.onerror = () => reject(new Error("The scanner did not load."));
    document.head.append(script);
  });
}

/// Scans a pairing code with the camera and pairs with its computer.
async function startScan() {
  show("t-scan");
  const attempt = scan.attempt;
  const state = view().querySelector("[data-scan-state]");
  const video = view().querySelector("video");
  try {
    await loadDecoder();
    const stream = await navigator.mediaDevices.getUserMedia({ video: { facingMode: "environment" }, audio: false });
    if (attempt !== scan.attempt) return stream.getTracks().forEach((track) => track.stop());
    scan.stream = stream;
    video.srcObject = stream;
    await video.play();
  } catch (error) {
    if (attempt !== scan.attempt) return;
    stopScan();
    state.textContent =
      error.name === "NotAllowedError"
        ? "Herdr may not use the camera. Allow it in Settings, then try again."
        : `The camera did not start: ${error.message}`;
    return;
  }
  const canvas = document.createElement("canvas");
  const context = canvas.getContext("2d", { willReadFrequently: true });
  const look = () => {
    if (!scan.stream) return;
    if (video.readyState >= video.HAVE_CURRENT_DATA && video.videoWidth) {
      // A code fills much of the frame; a smaller image decodes faster.
      const scale = Math.min(1, 720 / Math.max(video.videoWidth, video.videoHeight));
      canvas.width = Math.round(video.videoWidth * scale);
      canvas.height = Math.round(video.videoHeight * scale);
      context.drawImage(video, 0, 0, canvas.width, canvas.height);
      const image = context.getImageData(0, 0, canvas.width, canvas.height);
      const code = window.jsQR(image.data, image.width, image.height, { inversionAttempts: "dontInvert" });
      const link = code && pairingLink(code.data);
      if (link) {
        stopScan();
        history.replaceState(null, "", "/computers");
        return pair(link, true);
      }
      if (code) state.textContent = "That is not a Herdr pairing code.";
    }
    scan.timer = setTimeout(look, 150);
  };
  look();
}

function stopScan() {
  scan.attempt += 1;
  clearTimeout(scan.timer);
  scan.stream?.getTracks().forEach((track) => track.stop());
  scan.stream = null;
}

// ---- pairing ----

/// What the computer calls this phone, from what the browser says it is.
/// Pairing asks nothing: the code the desktop confirms tells phones apart.
function deviceName() {
  const agent = navigator.userAgent;
  if (/iPad/.test(agent) || (/Macintosh/.test(agent) && navigator.maxTouchPoints > 1)) return "iPad";
  if (/iPhone/.test(agent)) return "iPhone";
  if (/Android/.test(agent)) return /Mobile/.test(agent) ? "Android phone" : "Android tablet";
  return "Browser";
}

/// The pairing link a code holds, `https://ADDRESS/pair#t=TOKEN&fp=FINGERPRINT`,
/// as a URL; null for anything else.
function pairingLink(text) {
  try {
    const url = new URL(text);
    const params = new URLSearchParams(url.hash.slice(1));
    return url.protocol === "https:" && url.pathname === "/pair" && params.get("t") && params.get("fp") ? url : null;
  } catch {
    return null;
  }
}

/// Claims a scanned code at once and waits for the desktop's approval. The
/// code may name this page's computer, opened by the camera, or another
/// computer, scanned in the app to add it (`scanned`).
async function pair(link, scanned = false) {
  const params = new URLSearchParams(link.hash.slice(1));
  const origin = link.origin;
  const here = origin === location.origin;
  show("t-approve");
  // Leaving this view abandons the attempt: an approval that arrives after
  // must not switch computers under the phone.
  const shown = app.views;
  const left = () => app.views !== shown;
  const state = view().querySelector("[data-state]");
  view().querySelector("[data-approve-back]").hidden = !scanned;
  showTrust(origin, params.get("fp") || "", here);
  try {
    const pairKey = await crypto.subtle.generateKey({ name: "ECDSA", namedCurve: "P-256" }, false, ["sign"]);
    const publicKey = await crypto.subtle.exportKey("raw", pairKey.publicKey);
    const claim = await (
      await call("/api/pair/claim", {
        method: "POST",
        auth: false,
        origin,
        body: { token: params.get("t"), name: deviceName(), public_key: b64url(publicKey) },
      })
    ).json();
    if (left()) return;
    view().querySelector("[data-code]").textContent = claim.code.replace(/(\d{3})(\d{3})/, "$1 $2");
    state.textContent = "Waiting for approval…";
    for (;;) {
      await new Promise((resolve) => setTimeout(resolve, 1500));
      if (left()) return;
      const answer = await (await call(`/api/pair/claim/${claim.claim}`, { auth: false, origin })).json();
      if (left()) return;
      if (answer.state === "approved") {
        await adopt(pairKey, answer, params.get("fp"));
        // Straight home: its footer offers Add to Home Screen.
        history.replaceState(null, "", "/");
        return start();
      }
      if (answer.state !== "claimed") {
        state.textContent =
          answer.state === "denied" ? "Your computer declined. Scan a new code to try again." : "The code expired. Scan a new one.";
        return;
      }
    }
  } catch (failure) {
    report("pair", failure);
    if (left()) return;
    // A network failure here is almost always a certificate this phone no
    // longer trusts. Drop the cached app and load the page itself once, so
    // Safari shows its warning instead.
    if (here && failure instanceof TypeError && !sessionStorage.getItem(RELOADED)) {
      sessionStorage.setItem(RELOADED, "1");
      await dropCachedApp();
      return location.reload();
    }
    state.textContent =
      failure instanceof TypeError
        ? "This phone cannot make a secure connection to your computer. Install its certificate below, then scan a new code."
        : `${failure.message} Scan a new code to try again.`;
    if (failure instanceof TypeError) view().querySelector("[data-trust]")?.removeAttribute("hidden");
  }
}

/// Unregisters the service worker and deletes its cached files.
async function dropCachedApp() {
  try {
    const registrations = (await navigator.serviceWorker?.getRegistrations()) || [];
    await Promise.all(registrations.map((registration) => registration.unregister()));
    await Promise.all((await caches.keys()).map((key) => caches.delete(key)));
  } catch {
    // Best effort: the reload still tries the network first.
  }
}

/// Shows how to trust the computer at `origin`: on its own page only when
/// this browser does not trust it yet, as pairing works without it but the
/// installed app and its notifications need it; for another computer, only
/// once a claim fails, as the app cannot reach it at all untrusted.
async function showTrust(origin, fingerprint, here) {
  const box = view().querySelector("[data-trust]");
  if (!box) return;
  for (const link of box.querySelectorAll("a[data-computer]")) link.href = origin + link.getAttribute("href");
  box.querySelector("[data-fingerprint]").textContent = fingerprint.replace(/(.{2})(?=.)/g, "$1:");
  if (!here) return;
  const trusted = await navigator.serviceWorker
    ?.register("/sw.js", { scope: "/" })
    .then(() => true, () => false);
  if (!trusted) box.hidden = false;
}

/// Forgets the computer shown, and shows the next one that answers.
async function forget() {
  const host = app.device;
  if (host && ownsPush(host)) {
    try {
      await (await pushSubscription())?.unsubscribe();
    } catch {
      // No push subscription.
    }
    writeJson(localStorage, PUSH_HOST, null);
  }
  if (host) await dropKey(host).catch(() => {});
  // A pairing `migrate` has not moved yet would come back on the next launch.
  if (host && readJson(localStorage, DEVICE)?.id === host.id) {
    writeJson(localStorage, DEVICE, null);
    await keyStore("readwrite", (store) => store.delete("device")).catch(() => {});
  }
  saveHosts(hosts().filter((other) => other.fingerprint !== host?.fingerprint));
  app.session = null;
  app.origin = null;
  writeJson(localStorage, HANDOFF, null);
  writeJson(sessionStorage, TOKEN, null);
  if (app.device) return start(true);
  history.replaceState(null, "", "/");
  show("t-unpaired");
}

// ---- push ----

async function pushSubscription() {
  const registration = await navigator.serviceWorker?.ready;
  return registration?.pushManager?.getSubscription();
}

/// Whether this app's push subscription is `host`'s. One made before the
/// app held several computers belongs to the only one there was.
function ownsPush(host) {
  const owner = readJson(localStorage, PUSH_HOST);
  return owner ? owner === host.fingerprint : hosts().length <= 1;
}

/// The computer whose notifications this app gets, when it is not `host`.
async function pushElsewhere(host) {
  if (ownsPush(host) || !(await pushSubscription())) return null;
  const owner = readJson(localStorage, PUSH_HOST);
  return hosts().find((other) => other.fingerprint === owner) || { computer: "another computer" };
}

async function subscribePush() {
  const elsewhere = await pushElsewhere(app.device);
  if (elsewhere) throw new Error(`Notifications come from ${elsewhere.computer}. Turn them off there first.`);
  const registration = await navigator.serviceWorker.ready;
  const { key } = await (await api("/api/push/key")).json();
  const subscription = await registration.pushManager.subscribe({
    userVisibleOnly: true,
    applicationServerKey: fromB64url(key),
  });
  await api("/api/push", { method: "POST", body: subscription.toJSON() });
  writeJson(localStorage, PUSH_HOST, app.device.fingerprint);
}

async function unsubscribePush() {
  if (ownsPush(app.device)) {
    await (await pushSubscription())?.unsubscribe();
    writeJson(localStorage, PUSH_HOST, null);
  }
  await api("/api/push", { method: "DELETE" });
}

// ---- Alpine components (CSP build: no inline expressions) ----

document.addEventListener("alpine:init", () => {
  const Alpine = window.Alpine;
  Alpine.data("fold", () => ({
    open: true,
    toggle() {
      this.open = !this.open;
    },
    get openClass() {
      return this.open ? "" : "-rotate-90";
    },
  }));
  Alpine.data("history", () => ({
    loaded: false,
    get folded() {
      return !this.loaded;
    },
    async load() {
      const pane = this.$el.closest("[data-pane]").dataset.pane;
      this.$refs.body.innerHTML = await (await api(`/f/chat/${pane}/history`)).text();
      this.loaded = true;
    },
  }));
  Alpine.data("composer", () => ({
    files: [],
    uploading: 0,
    sending: false,
    // Whether the draft holds text or pictures, and whether the agent is
    // working; together they pick the Stop or Send button.
    draft: false,
    working: false,
    init() {
      this.working = "working" in this.$root.dataset;
    },
    refresh() {
      this.draft = this.$refs.text.value.trim() !== "" || this.files.length > 0;
    },
    // Alpine's CSP build reads properties, not expressions.
    get stopShown() {
      return this.working && !this.draft;
    },
    get sendShown() {
      return !this.working || this.draft;
    },
    get sendOff() {
      return !this.draft;
    },
    // Marks the composer while it holds pictures or work in flight, so a
    // live patch leaves it alone.
    mark() {
      const busy = this.files.length > 0 || this.uploading > 0 || this.sending;
      if (busy) this.$root.dataset.busy = "";
      else delete this.$root.dataset.busy;
    },
    pick() {
      this.$refs.file.click();
    },
    attach(event) {
      const files = [...event.target.files];
      event.target.value = "";
      this.upload(files);
    },
    // A screenshot copied on the phone arrives as a file a textarea cannot
    // hold; it becomes an attachment, and any text pasted with it still goes
    // into the draft.
    paste(event) {
      const files = [...(event.clipboardData?.items || [])]
        .filter((item) => item.kind === "file" && item.type.startsWith("image/"))
        .map((item) => item.getAsFile())
        .filter(Boolean);
      if (!files.length) return;
      if (!event.clipboardData.types.includes("text/plain")) event.preventDefault();
      this.upload(files);
    },
    async upload(files) {
      const pane = this.$root.closest("[data-pane]").dataset.pane;
      const picked = files.slice(0, 4 - this.files.length - this.uploading);
      if (picked.length < files.length) flash("A prompt holds at most 4 pictures.");
      this.uploading += picked.length;
      this.mark();
      for (const file of picked) {
        try {
          const { id } = await (await api(`/api/chat/${pane}/upload`, { method: "POST", raw: file })).json();
          this.files.push({ id, name: file.name || "Picture" });
        } catch (error) {
          flash(error.message);
        }
        this.uploading -= 1;
        this.chips();
      }
    },
    chips() {
      const chips = this.$refs.chips;
      chips.replaceChildren(
        ...this.files.map((file, index) => {
          const chip = document.createElement("button");
          chip.type = "button";
          chip.className = "chip";
          chip.textContent = `${file.name} ✕`;
          chip.addEventListener("click", () => {
            this.files.splice(index, 1);
            this.chips();
          });
          return chip;
        }),
      );
      this.mark();
      this.refresh();
    },
    grow() {
      const text = this.$refs.text;
      text.style.height = "auto";
      text.style.height = `${text.scrollHeight}px`;
      this.refresh();
      this.suggest();
    },
    // While the draft is "/" and a word, lists the first commands that
    // start with it.
    suggest() {
      const list = this.$refs.slash;
      if (!list) return;
      const typed = /^\/(\S*)$/.exec(this.$refs.text.value)?.[1]?.toLowerCase();
      let shown = 0;
      for (const button of list.children) {
        const match = typed !== undefined && shown < SLASH_SHOWN && button.dataset.slash.toLowerCase().startsWith(typed);
        button.hidden = !match;
        if (match) shown += 1;
      }
      list.hidden = shown === 0;
    },
    key(event) {
      // A hardware keyboard sends with Enter; on touch Enter is a newline.
      if (event.key === "Enter" && !event.shiftKey && matchMedia("(pointer: fine)").matches) {
        this.send(event);
      }
    },
    slash(event) {
      this.$refs.text.value = `/${event.currentTarget.dataset.slash} `;
      this.refresh();
      this.$refs.text.focus();
      this.suggest();
    },
    // The draft stays until the chat takes it, so a dropped connection
    // loses nothing; the user sends again.
    async send(event) {
      event.preventDefault();
      const text = this.$refs.text.value.trim();
      if ((!text && !this.files.length) || this.sending || this.uploading > 0) return;
      const attachments = this.files.map((file) => ({ path: file.id }));
      this.sending = true;
      this.mark();
      const taken = await command(this.$root.closest("[data-pane]").dataset.pane, { command: "prompt", text, attachments });
      this.sending = false;
      if (taken) {
        this.$refs.text.value = "";
        this.files = [];
        this.chips();
        this.grow();
      }
      this.mark();
    },
  }));
  Alpine.data("terminal", () => ({
    sheet: false,
    openSheet() {
      termInput()?.blur();
      this.sheet = true;
    },
    closeSheet() {
      this.sheet = false;
    },
  }));
  // The project picker: a full-screen list with a search field; the
  // chosen path goes into the hidden input the form sends.
  Alpine.data("picker", () => ({
    open() {
      this.$refs.page.hidden = false;
      this.$refs.search.value = "";
      this.filter();
      // Focus within the tap, so iOS raises the keyboard.
      this.$refs.search.focus();
    },
    close() {
      this.$refs.search.blur();
      this.$refs.page.hidden = true;
    },
    filter() {
      const words = this.$refs.search.value.toLowerCase().split(/\s+/).filter(Boolean);
      let any = false;
      for (const group of this.$refs.page.querySelectorAll("[data-group]")) {
        let shown = 0;
        for (const row of group.querySelectorAll("[data-value]")) {
          row.hidden = !words.every((word) => row.dataset.find.includes(word));
          if (!row.hidden) shown += 1;
        }
        group.hidden = shown === 0;
        any ||= shown > 0;
      }
      this.$refs.empty.hidden = any;
    },
    choose(event) {
      const row = event.currentTarget;
      const path = row.dataset.value;
      this.$refs.value.value = path;
      for (const other of this.$refs.page.querySelectorAll("[data-value]")) {
        other.setAttribute("aria-pressed", String(other.dataset.value === path));
      }
      const [name, parent] = row.querySelectorAll(".truncate");
      this.$refs.name.textContent = name.textContent;
      this.$refs.parent.textContent = parent.textContent;
      this.close();
    },
  }));
  Alpine.data("settings", () => ({
    push: false,
    pushHint: "",
    // Whether this browser can subscribe at all; the switch is off and
    // disabled otherwise, with a hint saying how to get notifications.
    pushOff: true,
    async init() {
      // iOS Safari has PushManager, but only a Home Screen app's service
      // worker can subscribe: in a tab, `pushManager` is missing.
      const registration = await navigator.serviceWorker?.ready;
      const ios = /iPhone|iPad/.test(navigator.userAgent);
      if (!registration?.pushManager || (ios && !installed())) {
        this.pushHint = ios
          ? "Add Herdr to your Home Screen and open it from there to get notifications."
          : "This browser cannot receive notifications.";
        return;
      }
      this.pushOff = false;
      const elsewhere = await pushElsewhere(app.device);
      this.push = !elsewhere && Boolean(await pushSubscription());
      if (elsewhere) this.pushHint = `Notifications come from ${elsewhere.computer}.`;
      else this.pushHint = this.push ? "Questions and finished turns arrive here." : "";
    },
    // Alpine's CSP build cannot assign through x-model, so the switch's
    // own state says what the user asked for.
    async togglePush(event) {
      const input = event.target;
      const wanted = input.checked;
      try {
        if (wanted) await subscribePush();
        else await unsubscribePush();
        this.push = wanted;
        this.pushHint = wanted ? "Questions and finished turns arrive here." : "";
      } catch (error) {
        this.push = !wanted;
        input.checked = !wanted;
        this.pushHint = error.message;
      }
    },
    forget() {
      if (confirm("Forget this computer? You will need to pair again.")) forget();
    },
  }));
});

// ---- start ----

/// Connects to the computer shown and opens the page the address names.
/// With `anyHost`, on launch or when asked to try again, another paired
/// computer is shown when that one does not answer; otherwise, as when a
/// stream drops, it stays on the computer shown.
async function start(anyHost = false) {
  await migrate();
  app.device = hosts()[0] || null;
  const link = location.pathname === "/pair" && pairingLink(location.href);
  if (link) return pair(link);
  // Every launch of the Home Screen app opens its start address; only the
  // first, before it holds a device, redeems the token there.
  const handoff = new URLSearchParams(location.search).get("handoff");
  if (handoff && !app.device) return redeemHandoff(handoff);
  if (handoff && installed()) history.replaceState(null, "", location.pathname);
  if (!app.device) {
    if (installed()) report("start", "this app holds no pairing and opened without a handoff");
    return show("t-unpaired");
  }
  show("t-loading");
  try {
    if (anyHost) {
      const { host, origin } = await pickHost();
      // Another computer has none of this one's chats or terminals.
      if (host.fingerprint !== app.device.fingerprint) history.replaceState(null, "", "/");
      useHost(host);
      app.origin = origin;
    } else {
      app.origin = await reach(app.device);
    }
    await signIn();
    route();
    refreshOrigins();
  } catch (error) {
    report("start", error);
    if (error.status === 403 || error.status === 404) return forget();
    show("t-offline");
  }
}

if ("serviceWorker" in navigator) navigator.serviceWorker.register("/sw.js", { scope: "/" }).catch(() => {});
// A hidden page closes its stream, so the computer shows the phone as
// gone at once rather than when a keep-alive finally fails; iOS suspends
// it soon anyway. Coming back reopens only the stream, whose first patch
// catches the view up without touching a composer holding a draft.
document.addEventListener("visibilitychange", () => {
  if (document.visibilityState === "hidden") return stopStream();
  if (!app.device || app.stream) return;
  const target = app.path === location.pathname && routeOf(location.pathname, location.search);
  if (target?.stream && app.session?.expires > Date.now() + 60_000) startStream(target.stream);
  else start();
});
navigator.serviceWorker?.addEventListener("message", (event) => {
  if (event.data?.navigate) navigate(event.data.navigate);
});
start(true);
