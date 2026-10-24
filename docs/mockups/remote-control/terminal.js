// Fills the repeated parts (terminal text, keyboard), then handles picks.
const SESSION = `<span class="p">~/code/herdr-gpui</span> <span class="g">feat/remote-control</span>
<span class="b">❯</span> cargo test -p herdr-remote-control
<span class="m">   Compiling herdr-remote-control v0.9.0</span>
<span class="m">    Finished \`test\` profile in 41.2s</span>
     Running unittests src/lib.rs
running 211 tests
test auth::tests::pairing_rotates ... <span class="g">ok</span>
test hub::tests::projection ... <span class="g">ok</span>
test push::tests::encrypt_rfc8291 ... <span class="g">ok</span>
test tls::tests::leaf_verifies ... <span class="g">ok</span>
test web::tests::stream::revoked ... <span class="g">ok</span>
test web::tests::origins::retired ... <span class="g">ok</span>
test web::tests::uploads::expire ... <span class="g">ok</span>
test server::tests::control_socket ... <span class="y">running</span>
<span class="m">^C</span>
<span class="b">❯</span> git status -sb
<span class="g">## feat/remote-control</span>...origin
 <span class="r">M</span> crates/herdr-remote-control/src/web.rs
 <span class="r">M</span> docs/mockups/remote-control/terminal.html
<span class="b">❯</span> <span class="cursor"></span>`;

const SHORT = SESSION.split("\n").slice(-9).join("\n");

const KEYS = [
  "q w e r t y u i o p",
  "a s d f g h j k l",
  "⇧ z x c v b n m ⌫",
];

function keyboard(element) {
  const prefix = element.classList.contains("prefix");
  const map = { c: "tab", v: "split│", "-": "split─", x: "close", z: "zoom", h: "←pane", j: "↓pane", k: "↑pane", l: "→pane", n: "next", p: "prev", w: "spaces" };
  const rows = KEYS.map((row) =>
    row.split(" ").map((key) => {
      const wide = key === "⇧" || key === "⌫";
      const action = prefix && map[key];
      return `<span class="k${wide ? " wide" : ""}${action ? " mapped" : ""}">${key}${action ? `<small>${action}</small>` : ""}</span>`;
    }).join(""),
  );
  const last = prefix
    ? `<span class="k wide">123</span><span class="k mapped">-<small>split─</small></span><span class="k space">space</span><span class="k wide">esc</span>`
    : `<span class="k wide">123</span><span class="k wide">🌐</span><span class="k space">space</span><span class="k wide">return</span>`;
  element.innerHTML = [...rows, last].map((row) => `<div class="krow">${row}</div>`).join("");
}

for (const term of document.querySelectorAll(".term[data-fill]")) {
  term.innerHTML = term.dataset.fill === "short" ? SHORT : SESSION;
}
document.querySelectorAll(".kbd").forEach(keyboard);

const chosen = { keys: "B", actions: "A", size: "D" };
const picks = { ...chosen };
const label = document.getElementById("picks");
const describe = () => Object.entries(picks).map(([k, v]) => `${k}: ${v}`).join(" · ");
for (const row of document.querySelectorAll(".row[data-group]")) {
  row.querySelector(`figure[data-variant="${chosen[row.dataset.group]}"]`)?.classList.add("picked");
  for (const figure of row.querySelectorAll("figure[data-variant]")) {
    const button = document.createElement("button");
    button.className = "hk-button pick";
    button.textContent = "Pick " + figure.dataset.variant;
    button.addEventListener("click", () => {
      row.querySelectorAll("figure").forEach((f) => f.classList.remove("picked"));
      figure.classList.add("picked");
      picks[row.dataset.group] = figure.dataset.variant;
      label.textContent = describe();
    });
    figure.appendChild(button);
  }
}

document.getElementById("send").addEventListener("click", () => {
  const notes = document.getElementById("notes").value.trim();
  const summary = "Mobile terminal picks: " + (describe() || "none") + (notes ? ` — ${notes}` : "");
  const data = { ...picks, notes };
  if (window.herdr && window.herdr.send(summary, data)) return;
  navigator.clipboard?.writeText(summary);
  label.textContent = "Copied to clipboard (not in Herdr UI).";
});

label.textContent = describe();

document.getElementById("theme").addEventListener("click", () => {
  document.documentElement.classList.toggle("light");
});
