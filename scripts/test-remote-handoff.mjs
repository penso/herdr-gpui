import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";

const source = readFileSync(new URL("../crates/herdr-remote-control/src/web/assets/app.js", import.meta.url), "utf8");
const functions = ["offerHandoff", "adopt"].map((name) => {
  const match = source.match(new RegExp(`^async function ${name}\\([^\\n]*\\) \\{[\\s\\S]*?^\\}`, "m"));
  assert.ok(match, `missing production function ${name}`);
  return match[0];
}).join("\n");

function browser(cached, request) {
  const storage = new Map(cached ? [["handoff", cached]] : []);
  const replacements = [];
  let requests = 0;
  const device = { id: "new-device", fingerprint: "AA:BB", origins: ["https://computer.test"] };
  const context = vm.createContext({
    app: { device, handoffBusy: false }, HANDOFF: "handoff", TOKEN: "session",
    localStorage: storage, sessionStorage: new Map(), URL, URLSearchParams,
    installed: () => false,
    readJson: (store, key) => store.get(key),
    writeJson: (store, key, value) => value === null ? store.delete(key) : store.set(key, value),
    api: async () => { requests++; return { json: async () => request ? request() : ({ token: "fresh", expires_ms: Date.now() + 86400000 }) }; },
    location: { origin: "https://computer.test", pathname: "/", search: "", replace: (url) => replacements.push(url) },
    document: { querySelector: () => ({ href: "https://computer.test/manifest.webmanifest?handoff=fresh" }) },
    history: { state: null, replaceState: (_, __, url) => replacements.push(url) },
    hosts: () => [], saveKey: async () => {}, dropKey: async () => {},
    useHost: (host) => { context.app.device = host; },
  });
  vm.runInContext(functions, context);
  return { context, storage, replacements, requests: () => requests };
}

test("a legacy consumed token is replaced after a fresh browser pairing", async () => {
  const b = browser({ token: "consumed", expires_ms: Date.now() + 86400000 });
  await vm.runInContext("offerHandoff()", b.context);
  assert.equal(b.requests(), 1);
  assert.equal(b.storage.get("handoff").device, "new-device");
  assert.deepEqual(b.replacements, ["/?handoff=fresh"]);
});

test("an offer for another device cannot reach this app's manifest", async () => {
  const b = browser({ token: "other", device: "old-device", fingerprint: "AA:BB", expires_ms: Date.now() + 86400000 });
  await vm.runInContext("offerHandoff()", b.context);
  assert.equal(b.requests(), 1);
  assert.equal(b.storage.get("handoff").token, "fresh");
});

test("an unused offer for this pairing stays valid across page reloads", async () => {
  const b = browser({ token: "fresh", device: "new-device", fingerprint: "AA:BB", expires_ms: Date.now() + 86400000 });
  await vm.runInContext("offerHandoff()", b.context);
  assert.equal(b.requests(), 0);
  assert.deepEqual(b.replacements, ["/?handoff=fresh"]);
});

test("accepting a new key clears the previous one-use offer even for the same device ID", async () => {
  const b = browser({ token: "old" });
  b.context.answer = { device: "new-device", fingerprint: "AA:BB", origins: ["https://computer.test"], computer: "Computer" };
  await vm.runInContext('adopt({}, answer, "AABB")', b.context);
  assert.equal(b.storage.has("handoff"), false);
});

test("concurrent navigations mint once and a late answer cannot cross pairings", async () => {
  let release;
  const response = new Promise((resolve) => { release = resolve; });
  const b = browser(null, () => response);
  const first = vm.runInContext("offerHandoff()", b.context);
  await vm.runInContext("offerHandoff()", b.context);
  assert.equal(b.requests(), 1);
  b.context.app.device = { id: "another-device", fingerprint: "CC:DD" };
  release({ token: "late", expires_ms: Date.now() + 86400000 });
  await first;
  assert.equal(b.storage.has("handoff"), false);
  assert.deepEqual(b.replacements, []);
  assert.equal(b.context.app.handoffBusy, false);
});
