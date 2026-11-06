/* Concept 04 · Atlas: renders the whole directory from assets/features.js with
   search, status and platform filters, a scroll-spied area nav and copy buttons. */
(function () {
  "use strict";

  var SHOTS = {
    workspaces: { src: "layouts.jpg", w: 1800, h: 1162, alt: "Five sidebar layouts, each with ledger-api and its nested feat/multi-currency and fix/rounding worktrees." },
    terminals: { src: "workspaces.jpg", w: 1800, h: 1125, alt: "An editor split beside a git log and a passing test run, with the agents sidebar listing claude, codex and pi." },
    git: { src: "review.jpg", w: 1800, h: 1125, alt: "Review changes tab with a diff of src/ledger.rs and two notes ready to send to the agent." },
    parallel: { src: "teleport.png", w: 1180, h: 640, alt: "Teleport dialog moving feat-multi-currency to build-box or gpu-runner over SSH." },
    browser: { src: "browser-annotate.jpg", w: 1800, h: 1125, alt: "A local docs page in a browser tab with two pinned notes listed in a Notes panel." },
    usage: { src: "usage.png", w: 1000, h: 800, alt: "Codex usage panel: weekly 97% left, resets in 4 days 6 hours." },
    look: { src: "theme-search.jpg", w: 1543, h: 1800, alt: "Theme picker with a search field and a list of themes with live preview." }
  };

  var PLATFORMS = [
    { id: "mac", label: "macOS" },
    { id: "linux", label: "Linux" },
    { id: "win", label: "Windows" },
    { id: "phone", label: "Phone" }
  ];

  /* Where a feature runs, derived from its caveat text in features.js. */
  function platformsOf(f) {
    var w = f.where || "";
    if (f.area === "phone") return ["phone"];
    if (/^macOS and Linux/.test(w)) return ["mac", "linux"];
    if (/^macOS and Windows/.test(w)) return ["mac", "win"];
    if (w === "Windows") return ["win"];
    return ["mac", "linux", "win"];
  }

  function el(tag, cls, text) {
    var n = document.createElement(tag);
    if (cls) n.className = cls;
    if (text != null) n.textContent = text;
    return n;
  }

  function chipGroup(root, items, onChange) {
    var state = "all";
    var all = [{ id: "all", label: "All" }].concat(items);
    all.forEach(function (it) {
      var b = el("button", "chip", it.label);
      b.type = "button";
      b.setAttribute("aria-pressed", it.id === "all" ? "true" : "false");
      if (it.cls) {
        var dot = el("span", "chip-dot " + it.cls);
        dot.setAttribute("aria-hidden", "true");
        b.prepend(dot);
      }
      b.addEventListener("click", function () {
        state = it.id;
        root.querySelectorAll(".chip").forEach(function (o) { o.setAttribute("aria-pressed", o === b ? "true" : "false"); });
        onChange(state);
      });
      root.appendChild(b);
    });
    return function () { return state; };
  }

  function build() {
    var S = window.Sheprd;
    if (!S) return;
    var areasRoot = document.getElementById("areas");
    var nav = document.getElementById("area-nav");
    var rows = [];

    S.AREAS.forEach(function (a, ai) {
      var feats = S.FEATURES.filter(function (f) { return f.area === a.id; });
      var sec = el("section", "area");
      sec.id = "area-" + a.id;
      sec.setAttribute("aria-labelledby", "h-" + a.id);
      var head = el("div", "area-head");
      var num = el("span", "area-num", String(ai + 1).padStart(2, "0"));
      num.setAttribute("aria-hidden", "true");
      var h = el("h2", null, a.name);
      h.id = "h-" + a.id;
      head.appendChild(num);
      head.appendChild(h);
      head.appendChild(el("p", "area-blurb", a.blurb));
      sec.appendChild(head);

      var body = el("div", "area-body" + (SHOTS[a.id] ? " has-shot" : ""));
      var ul = el("ul", "feat-list");
      feats.forEach(function (f) {
        var li = el("li", "feat");
        var plats = platformsOf(f);
        li.dataset.status = f.status;
        li.dataset.platforms = plats.join(" ");
        var top = el("div", "feat-top");
        top.appendChild(el("h3", "feat-name", f.name));
        top.appendChild(S.badge(f.status));
        li.appendChild(top);
        li.appendChild(el("p", "feat-desc", f.desc));
        var meta = el("p", "feat-meta");
        var pl = el("span", "feat-plats");
        pl.setAttribute("aria-label", "Runs on: " + plats.map(function (p) { return PLATFORMS.find(function (x) { return x.id === p; }).label; }).join(", "));
        PLATFORMS.forEach(function (p) {
          if (p.id === "phone" && plats.indexOf("phone") === -1) return;
          if (p.id !== "phone" && plats.indexOf("phone") !== -1) return;
          var s = el("span", "plat" + (plats.indexOf(p.id) !== -1 ? " on" : ""), p.label);
          s.setAttribute("aria-hidden", "true");
          pl.appendChild(s);
        });
        meta.appendChild(pl);
        if (f.where) meta.appendChild(el("span", "feat-where", f.where));
        li.appendChild(meta);
        li._text = (f.name + " " + f.desc + " " + f.where + " " + a.name).toLowerCase();
        rows.push(li);
        ul.appendChild(li);
      });
      body.appendChild(ul);

      var shot = SHOTS[a.id];
      if (shot) {
        var fig = el("figure", "area-shot");
        var img = el("img");
        img.src = "../../assets/shots/" + shot.src;
        img.width = shot.w;
        img.height = shot.h;
        img.loading = "lazy";
        img.alt = shot.alt;
        fig.appendChild(img);
        var cap = el("figcaption");
        cap.appendChild(el("span", "tag-screenshot", "Real screenshot"));
        cap.appendChild(document.createTextNode(" Current build, Herdr UI branding."));
        fig.appendChild(cap);
        body.appendChild(fig);
      }
      sec.appendChild(body);
      areasRoot.appendChild(sec);

      var li = el("li");
      var link = el("a");
      link.href = "#area-" + a.id;
      link.appendChild(el("span", null, a.name));
      var c = el("span", "nav-count", String(feats.length));
      c.setAttribute("aria-label", feats.length + " features");
      link.appendChild(c);
      li.appendChild(link);
      nav.appendChild(li);
    });

    // Stats
    var counts = S.counts();
    var stats = document.getElementById("stats");
    [["Features", counts.total], ["Shipped", counts.shipped], ["Opt-in", counts.optin], ["Beta or experimental", counts.testflight + counts.experimental], ["Planned", counts.planned]].forEach(function (p) {
      var d = el("div", "stat");
      d.appendChild(el("dt", null, p[0]));
      d.appendChild(el("dd", null, String(p[1])));
      stats.appendChild(d);
    });

    // Filters
    var q = document.getElementById("q");
    var statusItems = Object.keys(S.STATUS).map(function (k) { return { id: k, label: S.STATUS[k].label, cls: S.STATUS[k].cls }; });
    var getStatus = chipGroup(document.getElementById("status-filter"), statusItems, apply);
    var getPlat = chipGroup(document.getElementById("platform-filter"), PLATFORMS, apply);
    var result = document.getElementById("result");
    var empty = document.getElementById("empty");

    function apply() {
      var text = q.value.trim().toLowerCase();
      var st = getStatus();
      var pf = getPlat();
      var shown = 0;
      rows.forEach(function (r) {
        var ok = (st === "all" || r.dataset.status === st) &&
          (pf === "all" || r.dataset.platforms.split(" ").indexOf(pf) !== -1) &&
          (!text || r._text.indexOf(text) !== -1);
        r.hidden = !ok;
        if (ok) shown++;
      });
      document.querySelectorAll(".area").forEach(function (sec) {
        var any = !!sec.querySelector(".feat:not([hidden])");
        sec.hidden = !any;
        var a = nav.querySelector('a[href="#' + sec.id + '"]');
        if (a) a.parentNode.classList.toggle("is-empty", !any);
      });
      var filtered = text || st !== "all" || pf !== "all";
      result.textContent = filtered ? "Showing " + shown + " of " + rows.length + " features" : "";
      empty.hidden = shown !== 0;
    }
    q.addEventListener("input", apply);
    q.addEventListener("keydown", function (e) {
      if (e.key === "Escape" && q.value) { q.value = ""; apply(); }
    });
    document.addEventListener("keydown", function (e) {
      var t = e.target;
      var typing = t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.isContentEditable);
      if (e.key === "/" && !typing) { e.preventDefault(); q.focus(); }
    });
    apply();

    // Scroll spy: mark the area currently in view.
    if ("IntersectionObserver" in window) {
      var links = nav.querySelectorAll("a");
      var io = new IntersectionObserver(function (entries) {
        entries.forEach(function (en) {
          if (!en.isIntersecting) return;
          links.forEach(function (l) {
            var on = l.getAttribute("href") === "#" + en.target.id;
            if (on) l.setAttribute("aria-current", "true"); else l.removeAttribute("aria-current");
          });
        });
      }, { rootMargin: "-20% 0px -70% 0px" });
      document.querySelectorAll(".area").forEach(function (s) { io.observe(s); });
    }
  }

  function initCopy() {
    document.querySelectorAll(".copy[data-copy]").forEach(function (b) {
      var label = b.textContent;
      b.addEventListener("click", function () {
        if (!navigator.clipboard) { b.textContent = "Select the command"; return; }
        navigator.clipboard.writeText(b.dataset.copy).then(function () {
          b.textContent = "Copied";
          setTimeout(function () { b.textContent = label; }, 1600);
        }, function () { b.textContent = "Select the command"; });
      });
    });
  }

  document.addEventListener("DOMContentLoaded", function () {
    build();
    initCopy();
  });
})();
