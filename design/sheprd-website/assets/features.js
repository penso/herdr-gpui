/* Sheprd feature directory: the one source every concept renders from.
   Each entry is backed by a row in ../FEATURE-INVENTORY.md (source refs live there).
   status: shipped (in releases, on by default) | optin (in releases, off by default)
           | experimental (docs or UI say so) | testflight (iOS beta builds) | planned
   where:  platform caveat, empty when it works on macOS, Linux and Windows. */
(function () {
  "use strict";

  var STATUS = {
    shipped: { label: "Shipped", cls: "st-shipped" },
    optin: { label: "Opt-in", cls: "st-optin" },
    experimental: { label: "Experimental", cls: "st-experimental" },
    testflight: { label: "TestFlight beta", cls: "st-testflight" },
    planned: { label: "Planned", cls: "st-planned" }
  };

  var AREAS = [
    { id: "terminals", name: "Terminals & agents", blurb: "Native panes for every agent CLI you already use." },
    { id: "chat", name: "Native agent chat", blurb: "ACP chats drawn as a real chat, not a terminal." },
    { id: "workspaces", name: "Workspaces & worktrees", blurb: "One row per checkout, with the status that matters." },
    { id: "parallel", name: "Parallel work", blurb: "Dispatch, fan out, teleport and roll back." },
    { id: "git", name: "Review & Git", blurb: "Read the diff, leave notes, commit, push, open the PR." },
    { id: "orchestrate", name: "Issues & pull requests", blurb: "From an issue to an agent in one click." },
    { id: "hosts", name: "Hosts & cloud", blurb: "Local, SSH, WSL, containers and cloud machines." },
    { id: "browser", name: "Browser & previews", blurb: "Pages beside the terminal, with notes for the agent." },
    { id: "usage", name: "Usage & system", blurb: "Plan limits and machine load where you can see them." },
    { id: "look", name: "Look & feel", blurb: "Themes, layouts, fonts and keyboard control." },
    { id: "extend", name: "Mods, automations & CLI", blurb: "Extend the app; let agents drive it." },
    { id: "phone", name: "Phone, tablet & watch", blurb: "Answer your agents away from the desk." },
    { id: "trust", name: "Updates & trust", blurb: "Signed updates, keychain secrets, logs that stay local." }
  ];

  // [area, name, description, status, where]
  var ROWS = [
    ["terminals", "Native terminal painting", "Draws terminal cells, splits, colors and cursors natively in Rust with GPUI: no embedded emulator, no wrapped TUI.", "shipped", ""],
    ["terminals", "Two terminal backends", "Herdr 1 (herdr server) sessions and Herdr 2 (herdr-core) terminals side by side in the same workspaces; a pinned herdr-core ships with the app.", "shipped", ""],
    ["terminals", "Sessions outlive the window", "The terminal server owns every process. Close, detach or update the app and your agents keep running.", "shipped", ""],
    ["terminals", "Agent status everywhere", "Idle, working, blocked and done dots on every tab, row and badge, from the server's own snapshot.", "shipped", ""],
    ["terminals", "Find, copy mode, smooth scroll", "Scrollback search, a Vim-like keyboard copy mode, pixel-smooth scrolling and OSC 52 clipboard.", "shipped", ""],
    ["terminals", "Images and file drops", "Paste or drop images into agents; dropped files are uploaded first on SSH hosts.", "shipped", "Uploads: macOS and Linux"],
    ["terminals", "Annotate terminal text", "Select output, write a note, send it to the agent in that pane.", "shipped", ""],
    ["terminals", "Accessible terminal", "Terminal text exposed to VoiceOver, Narrator and NVDA, with dictation and IME composition.", "shipped", ""],
    ["chat", "ACP chats", "Claude, Codex, Copilot, Gemini, OpenCode or Pi over the Agent Client Protocol, drawn natively: reasoning, tool diffs, plans, permissions, context and cost.", "shipped", "Local and SSH hosts"],
    ["chat", "Chat layouts", "Timeline, Bubbles, Terminal, Document and Cards views of the same conversation.", "shipped", ""],
    ["chat", "Rich messages", "Markdown, equations, Mermaid, images and small interactive cards agents can send.", "shipped", "Live HTML: macOS and Windows"],
    ["workspaces", "Worktree sidebar", "Linked worktrees nest under their checkout with PR number, diff counts, ahead/behind and agent status.", "shipped", ""],
    ["workspaces", "New worktree, agent included", "Name a branch or pick a PR or issue; the new checkout opens with claude or codex already running.", "shipped", "Agent start: macOS and Linux"],
    ["workspaces", "Worktree scripts and notes", "Trusted setup, run and archive scripts per repo, and one-line notes on each checkout.", "shipped", ""],
    ["workspaces", "Overview: what needs you", "Agents waiting on you, failed runs and finished work to review, with snooze, pin and reopen.", "shipped", ""],
    ["workspaces", "Command palette", "Double-Shift palette over workspaces, agents, commands and projects with live pane preview.", "shipped", ""],
    ["workspaces", "Go to File, Go to Symbol", "Fast symbol and file search over local checkouts; open in your editor or a code tab.", "shipped", ""],
    ["workspaces", "Editor groups", "Side-by-side groups with draggable tabs, restored with the window.", "shipped", ""],
    ["parallel", "Smart Dispatch", "Ranks connected machines by spare cores and load, then creates the worktree on the best one.", "shipped", "macOS and Linux clients"],
    ["parallel", "Fan-out", "One prompt across several agents and machines; compare the results, keep one, clean up the rest.", "shipped", "macOS and Linux clients"],
    ["parallel", "Teleport", "Move a worktree, its tabs, running programs and agent sessions to another machine, and back.", "shipped", "macOS and Linux clients"],
    ["parallel", "Agent checkpoints", "A hidden Git snapshot at every agent turn, newest 50 per branch; diff or restore any of them.", "shipped", "Windows clients: not for SSH hosts"],
    ["parallel", "Move panes between devices", "Move a pane to a new tab, workspace or another machine with its agent session.", "shipped", "macOS and Linux clients"],
    ["git", "Review Changes", "PR-style diff of uncommitted or branch changes, unified or split, with notes you send to the agent.", "shipped", "Local checkouts"],
    ["git", "Commit, push, open the PR", "The title-bar Git menu commits, pushes, creates and merges pull requests and shows checks.", "shipped", ""],
    ["git", "PR badges", "Each worktree shows its PR colored by checks, reviews and merge state.", "shipped", "Needs GitHub sign-in"],
    ["git", "Code search index", "A heuristic symbol index across Rust, Swift, Go, Python, TypeScript, Java and more.", "shipped", ""],
    ["orchestrate", "Issues across repositories", "GitHub, GitLab and Beads issues from every repository in the window, filtered and sorted.", "shipped", ""],
    ["orchestrate", "Pull request board", "PRs grouped by what they wait on: your review, yours to fix, ready to merge. Comment and merge in place.", "shipped", "GitHub; GitLab MRs not yet"],
    ["orchestrate", "Dispatch an agent from an issue", "Pick agent, model and branch; a worktree is created and the agent starts. Or run a read-only PR review.", "shipped", ""],
    ["orchestrate", "Security inbox", "Dependabot alerts and private vulnerability reports across your repositories, most severe first.", "shipped", "github.com"],
    ["orchestrate", "GitHub and GitLab sign-in", "Device flow, the GitHub CLI or a token; several accounts; GitLab OAuth or tokens, self-managed too.", "shipped", ""],
    ["hosts", "Saved SSH machines", "Every machine in one sidebar, one shared login per host, MFA hosts through your ControlMaster.", "shipped", "Windows: no connection sharing"],
    ["hosts", "All Devices view", "Spaces and agents across every machine, with a two-hour activity heatmap per device.", "shipped", ""],
    ["hosts", "Nearby machine suggestions", "Suggests hosts from your SSH config, Tailscale peers and Bonjour.", "shipped", ""],
    ["hosts", "Ports and forwarding", "Listening ports per workspace; click to open, tunnelled automatically from remote machines.", "shipped", "Forwarding: macOS and Linux"],
    ["hosts", "WSL distributions", "WSL distributions as devices, with load, usage, ports and checkpoints.", "shipped", "Windows"],
    ["hosts", "Containers per agent", "Isolated Docker or Podman containers with limits and idle auto-stop, built from your project's image.", "shipped", "macOS and Linux clients"],
    ["hosts", "Coder and Azure", "Create Coder workspaces and Azure VMs or Container Apps sandboxes as devices.", "shipped", "macOS and Linux clients"],
    ["hosts", "More cloud sandboxes", "Daytona, Fly Sprites, Boat, Railway and self-hosted OpenSandbox.", "experimental", "macOS and Linux clients"],
    ["browser", "Browser tabs", "Web pages beside your terminals; agents open their previews there with one command.", "shipped", "macOS and Windows"],
    ["browser", "Page annotations and picks", "Pin notes to elements, text or regions and send them to the agent; mockups return the variant you pick.", "shipped", "macOS and Windows"],
    ["browser", "VS Code and px0 tabs", "VS Code for the Web and the px0 review workbench as tabs in a space.", "shipped", "macOS and Windows"],
    ["usage", "Plan usage for 85+ AI services", "Claude, Codex, Copilot, Cursor, Gemini, OpenRouter and many more: closest-to-limit in the status bar, details on click.", "shipped", ""],
    ["usage", "Machine load", "CPU, memory, disk and uptime per machine, read over SSH for remote hosts.", "shipped", ""],
    ["usage", "GPU and network quality", "GPU and VRAM load, and latency and loss in the status bar.", "optin", ""],
    ["usage", "Keep awake", "One click keeps the machine awake while agents work, or for a set time from the CLI.", "shipped", ""],
    ["look", "Hundreds of themes", "Built-ins plus every Ghostty theme, live preview, favorites, light and dark pairs that follow the OS.", "shipped", ""],
    ["look", "Sidebar and tab layouts", "Comfortable, compact, superset, orca, minimal and more; seven tab styles; status indicator sets.", "shipped", ""],
    ["look", "Fonts, backdrop, contrast", "Four font roles, translucent or picture backdrops, Mica on Windows 11, and a contrast floor for app marks.", "shipped", ""],
    ["look", "Rebind everything", "A shortcuts editor with per-platform profiles and Herdr prefix chords.", "shipped", ""],
    ["look", "Languages", "Fluent catalogs, RTL-ready layout and pseudo-locales. English ships today.", "planned", "Translations planned"],
    ["extend", "Mods in Luau", "Sandboxed scripts that add status items, sidebar sections, panels, palette commands or whole apps, live-reloaded.", "shipped", ""],
    ["extend", "Automations", "Scheduled Luau jobs that pull, test, start agents and notify, run only after you approve the exact script.", "shipped", ""],
    ["extend", "A CLI for agents", "About 60 commands over a local socket: open pages, read notes, checkpoints, teleport, cloud, usage. --json everywhere.", "shipped", ""],
    ["extend", "Managed agent skills", "Installs skills so Claude Code and other agents know how to use the app, with your consent.", "shipped", ""],
    ["phone", "Remote control from any phone", "An installable web app served from your computer: follow chats, answer questions, get Web Push.", "optin", "Any modern phone browser"],
    ["phone", "Pairing approved on both screens", "QR plus a matching code, then Allow on the desktop. Its own certificate authority, over LAN or Tailscale.", "optin", ""],
    ["phone", "One phone, every machine", "Approving a phone also enrols it on the SSH and cloud machines your computer uses.", "optin", ""],
    ["phone", "Native iPhone and iPad app", "Merged home across computers, chat, composer with dictation, terminal with key rows, iPad split view.", "testflight", "iOS 26"],
    ["phone", "Live Activities, widgets, Watch", "Lock-screen progress, Needs-You widgets, Controls, Siri, and an Apple Watch companion.", "experimental", "TestFlight builds"],
    ["phone", "Nearby pairing", "Pair a phone nearby over Bonjour or Bluetooth, no camera needed.", "experimental", "Bluetooth needs device testing"],
    ["phone", "Android app", "Android uses the web app today; a native app is not built.", "planned", ""],
    ["trust", "Signed in-app updates", "Ed25519-signed manifests, separate download and install approvals, Homebrew, APT, DNF and pacman aware.", "shipped", "Windows: when signed"],
    ["trust", "Secrets in your OS keychain", "Keychain, Secret Service or Credential Manager, or a private file if you opt in. Teleport can lend your GitHub CLI token to a destination that cannot reach GitHub, and its review says so first.", "shipped", ""],
    ["trust", "Logs and crash reports", "A local log window and redacted diagnostic bundles. Nothing is uploaded.", "shipped", ""],
    ["trust", "Verifiable releases", "Checksums, Sigstore signatures, build provenance and an SBOM on every release.", "shipped", ""]
  ];

  var FEATURES = ROWS.map(function (r, i) {
    return { id: "f" + i, area: r[0], name: r[1], desc: r[2], status: r[3], where: r[4] };
  });

  function el(tag, cls, text) {
    var n = document.createElement(tag);
    if (cls) n.className = cls;
    if (text != null) n.textContent = text;
    return n;
  }

  function badge(status) {
    var s = STATUS[status];
    return el("span", "st " + s.cls, s.label);
  }

  /* Render a searchable, filterable directory into root. Concepts style the
     .fd-* classes; the markup stays semantic so keyboard and screen readers work. */
  function render(root, opts) {
    opts = opts || {};
    var bar = el("div", "fd-bar");
    var label = el("label", "fd-search");
    var span = el("span", "sr-only", "Search features");
    var input = el("input");
    input.type = "search";
    input.placeholder = opts.placeholder || "Search " + FEATURES.length + " features…";
    input.autocomplete = "off";
    label.appendChild(span);
    label.appendChild(input);
    bar.appendChild(label);

    var chips = el("div", "fd-chips");
    chips.setAttribute("role", "group");
    chips.setAttribute("aria-label", "Filter by status");
    var active = "all";
    ["all", "shipped", "optin", "experimental", "testflight", "planned"].forEach(function (k) {
      var b = el("button", "fd-chip", k === "all" ? "All" : STATUS[k].label);
      b.type = "button";
      b.dataset.status = k;
      b.setAttribute("aria-pressed", k === "all" ? "true" : "false");
      b.addEventListener("click", function () {
        active = k;
        chips.querySelectorAll("button").forEach(function (o) { o.setAttribute("aria-pressed", o === b ? "true" : "false"); });
        apply();
      });
      chips.appendChild(b);
    });
    bar.appendChild(chips);
    root.appendChild(bar);

    var count = el("p", "fd-count");
    count.setAttribute("aria-live", "polite");
    root.appendChild(count);

    var groups = el("div", "fd-groups");
    var items = [];
    AREAS.forEach(function (a) {
      var sec = el(opts.collapsible ? "details" : "section", "fd-group");
      sec.dataset.area = a.id;
      if (opts.collapsible) sec.open = !!opts.open;
      var head = el(opts.collapsible ? "summary" : "div", "fd-group-head");
      var h = el("h3", "fd-group-title", a.name);
      var n = FEATURES.filter(function (f) { return f.area === a.id; }).length;
      head.appendChild(h);
      head.appendChild(el("span", "fd-group-count", n + ""));
      if (!opts.collapsible) head.appendChild(el("p", "fd-group-blurb", a.blurb));
      sec.appendChild(head);
      var ul = el("ul", "fd-list");
      FEATURES.filter(function (f) { return f.area === a.id; }).forEach(function (f) {
        var li = el("li", "fd-item");
        li.dataset.status = f.status;
        var top = el("div", "fd-item-top");
        top.appendChild(el("strong", "fd-name", f.name));
        top.appendChild(badge(f.status));
        li.appendChild(top);
        li.appendChild(el("p", "fd-desc", f.desc));
        if (f.where) li.appendChild(el("p", "fd-where", f.where));
        li._text = (f.name + " " + f.desc + " " + f.where + " " + a.name).toLowerCase();
        items.push(li);
        ul.appendChild(li);
      });
      sec.appendChild(ul);
      groups.appendChild(sec);
    });
    root.appendChild(groups);

    function apply() {
      var q = input.value.trim().toLowerCase();
      var shown = 0;
      items.forEach(function (li) {
        var ok = (active === "all" || li.dataset.status === active) && (!q || li._text.indexOf(q) !== -1);
        li.hidden = !ok;
        if (ok) shown++;
      });
      groups.querySelectorAll(".fd-group").forEach(function (g) {
        var any = g.querySelector(".fd-item:not([hidden])");
        g.hidden = !any;
        if (opts.collapsible && (q || active !== "all") && any) g.open = true;
      });
      count.textContent = shown + " of " + items.length + " features";
    }
    input.addEventListener("input", apply);
    apply();
    return { input: input };
  }

  function counts() {
    var c = { total: FEATURES.length };
    Object.keys(STATUS).forEach(function (k) { c[k] = FEATURES.filter(function (f) { return f.status === k; }).length; });
    return c;
  }

  window.Sheprd = {
    STATUS: STATUS,
    AREAS: AREAS,
    FEATURES: FEATURES,
    badge: badge,
    counts: counts,
    renderDirectory: render,
    /* Measured at commit 8e03793da with tokei over git-tracked first-party files;
       see ../FEATURE-INVENTORY.md "Codebase measurement". */
    METRICS: {
      commit: "8e03793da",
      crates: 26,
      rustProdLoc: 378965,
      rustTestLoc: 224291,
      rustTests: 8493,
      swiftLoc: 18900,
      usageProviders: 87,
      cliCommands: "about 60"
    }
  };
})();
