// Optional browser-controller regression checks, using Node's built-in runner.
// Execute the shipped functions against a minimal DOM; no copied scroll logic.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";

const source = readFileSync(new URL("../crates/herdr-remote-control/src/web/assets/app.js", import.meta.url), "utf8");
const functions = ["scrollChatToEnd", "handleEvent"].map((name) => {
  const match = source.match(new RegExp(`^function ${name}\\([^\\n]*\\) \\{[\\s\\S]*?^\\}`, "m"));
  assert.ok(match, `missing production function ${name}`);
  return match[0];
}).join("\n");

function browser({ count = 0, height = 844, y = 0 } = {}) {
  const calls = [];
  const body = { scrollHeight: height };
  const entries = {
    childElementCount: count,
    insertAdjacentHTML() { this.childElementCount++; body.scrollHeight = 1600; },
  };
  const root = { dataset: {}, contains: () => true };
  const context = vm.createContext({
    document: { body, getElementById: (id) => ({ "chat-turn": {}, "chat-entries": entries })[id] },
    window: { innerHeight: 800, scrollY: y, scrollTo: (x, y) => calls.push([x, y]) },
    navigator: {}, view: () => root, applySearch() {}, markRow() {},
  });
  vm.runInContext(functions, context);
  return { calls, context };
}

test("entering an empty chat resets inherited scroll even with safe-area padding", () => {
  const { calls, context } = browser({ y: 44 });
  vm.runInContext("scrollChatToEnd()", context);
  assert.deepEqual(calls, [[0, 0]]);
});

test("empty status patches do not scroll the header offscreen", () => {
  const { calls, context } = browser();
  vm.runInContext('handleEvent(\'event: patch\\ndata: {"regions":{}}\')', context);
  assert.deepEqual(calls, [[0, 0]]);
});

test("a populated conversation opens at its latest entry", () => {
  const { calls, context } = browser({ count: 2, height: 2000 });
  vm.runInContext("scrollChatToEnd()", context);
  assert.deepEqual(calls, [[0, 2000]]);
});

test("the first appended entry starts following from an empty chat", () => {
  const { calls, context } = browser();
  vm.runInContext('handleEvent(\'event: patch\\ndata: {"append":{"chat-entries":"<p>Answer</p>"}}\')', context);
  assert.deepEqual(calls, [[0, 1600]]);
});

test("streaming does not pull a reader away from earlier messages", () => {
  const { calls, context } = browser({ count: 3, height: 2000, y: 100 });
  vm.runInContext('handleEvent(\'event: patch\\ndata: {"regions":{}}\')', context);
  assert.deepEqual(calls, []);
});
