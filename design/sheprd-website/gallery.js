(function () {
  "use strict";

  // One entry per concept: [slug, name, idea, audience, look, structure, hero, breadth, scan, upkeep, tradeoff]
  var CONCEPTS = [
    ["01-field-guide", "Field Guide", "An editorial long read: numbered chapters, real screenshots as captioned figures, and the feature list as an appendix.", "Developers who read before they install", "Serif, cream paper, ink and orange rules", "Essay, then chapters, then appendix", "A headline, a standfirst, and one figure", 4, 2, 4, "Slow to scan, and only as good as its writing."],
    ["02-control-room", "Control Room", "A dark operations console. The hero is a live agent board, followed by KPIs and a dense table of features.", "Power users running many agents at once", "Dark, monospace, status colors", "Board, metrics, panels, table", "An agent board animating through states", 5, 4, 3, "Dense, and can intimidate newcomers."],
    ["03-the-loop", "The Loop", "A Swiss-style rail you step through, from issue to worktree, agent, review, checkpoint and pull request. Breadth comes after the loop.", "Tech leads and teams", "Clean grotesk, white, orange accent", "Workflow stepper, then everything around it", "One big promise plus the six-step rail", 3, 5, 4, "Workflow first, so breadth arrives later."],
    ["04-atlas", "Atlas", "The homepage is the searchable feature directory, with navigation by area, a status legend and a platform matrix.", "Evaluators comparing tools", "Documentation-like, neutral, compact", "Sidebar areas, search, matrix", "A short line, then straight into features", 5, 4, 5, "A weak emotional pitch."],
    ["05-bento", "Bento", "Bright modular tiles for launch-day energy. Each tile is one capability, with a screenshot crop.", "A broad developer audience, and social sharing", "Rounded type, warm gradients, big tiles", "Bento grid, then a collapsible directory", "A bold headline over the tile wall", 4, 5, 3, "Tiles can make deep features look trivial."],
    ["06-prompt", "Prompt", "The site is a terminal session. Features are presented as the app's real CLI commands, and the directory reads like `ls -l`.", "CLI natives and people building agents", "Near-black, amber and green, monospace", "Typed commands as sections", "`sheprd --help` typing out", 4, 3, 3, "A niche look that is harder for skimmers."],
    ["07-everywhere", "Everywhere", "Starts from where the work runs: your desk, your machines, your pocket. Then a platform matrix.", "Developers who work across several machines or remotely", "Airy, sky and slate, humanist sans", "Desk, machines, pocket, matrix", "A constellation of the user's devices", 4, 4, 4, "The phone story leans on pieces still in beta."],
    ["08-spec-sheet", "Spec Sheet", "A brutalist engineering datasheet: an architecture diagram, the trust model, release integrity and codebase metrics with their method.", "Contributors and security reviewers", "Off-white, black grid, orange rules", "Numbered spec tables", "Specs before adjectives", 5, 3, 4, "Cold, and the numbers can read as bragging."],
    ["09-the-flock", "The Flock", "A warm, illustrated “herd your agents” story. A paw-print trail leads through plain-language chapters.", "Newcomers and indie developers", "Cream, orange and sage, rounded, illustrated", "A story trail, explainers, directory", "A field of agents shown as a flock", 3, 4, 3, "Whimsy can undercut seriousness."],
    ["10-keynote", "Keynote", "A cinematic scroll: one sticky app window that changes screenshot with each chapter.", "First-time visitors", "Black, huge type, a single window", "Scroll-driven chapters", "One window that becomes every feature", 4, 4, 3, "Heavy on motion and screenshots."]
  ];

  // Filled in after all ten were built and reviewed.
  var RECOMMEND = [
    ["03-the-loop", "Best homepage. The issue to PR rail gives a newcomer the whole product in one interaction, and its six steps carry real screenshots and caveats. It stays clean at every width, and its type and grid are the easiest to keep consistent as features grow."],
    ["10-keynote", "Best first impression. Real screenshots do the persuading, the sticky window makes it feel like a native app, and it still ends in the full directory. It works well for a launch, but costs the most to keep current."],
    ["04-atlas", "Best companion page whichever homepage wins. Make it sheprd.org/features: search, status and platform filters, and the platform matrix answer “does it do X on my OS?” honestly. It also fits the brief to list everything."]
  ];

  var grid = document.getElementById("grid");
  var cmp = document.getElementById("cmp");
  var picked = null;
  var shortlist = [];

  function el(tag, cls, text) {
    var n = document.createElement(tag);
    if (cls) n.className = cls;
    if (text != null) n.textContent = text;
    return n;
  }
  function dots(n) {
    var s = el("span", "dots");
    s.setAttribute("aria-label", n + " out of 5");
    s.textContent = "●".repeat(n);
    var rest = el("span", null, "●".repeat(5 - n));
    rest.setAttribute("aria-hidden", "true");
    s.appendChild(rest);
    return s;
  }
  function href(c) { return "concepts/" + c[0] + "/index.html"; }

  CONCEPTS.forEach(function (c, i) {
    var num = c[0].slice(0, 2);
    var li = el("li", "g-card");
    li.id = "c" + num;
    var thumb = el("a", "g-thumb");
    thumb.href = href(c);
    thumb.setAttribute("aria-label", "Open concept " + num + ", " + c[1]);
    var img = el("img");
    img.src = "assets/thumbs/" + c[0] + ".jpg";
    img.alt = "";
    img.width = 1440; img.height = 900;
    img.loading = i < 3 ? "eager" : "lazy";
    thumb.appendChild(img);
    thumb.appendChild(el("span", "g-num", num));
    if (RECOMMEND.some(function (r) { return r[0] === c[0]; })) thumb.appendChild(el("span", "g-rectag", "Recommended"));
    li.appendChild(thumb);

    var body = el("div", "g-body");
    body.appendChild(el("h3", null, num + " · " + c[1]));
    body.appendChild(el("p", "g-idea", c[2]));
    var dl = el("dl", "g-meta");
    [["For", c[3]], ["Look", c[4]], ["Tradeoff", c[10]]].forEach(function (p) {
      dl.appendChild(el("dt", null, p[0]));
      dl.appendChild(el("dd", null, p[1]));
    });
    body.appendChild(dl);

    var act = el("div", "g-actions");
    var open = el("a", "btn btn-primary", "Open concept");
    open.href = href(c);
    act.appendChild(open);
    var pick = el("button", "btn", "Pick");
    pick.type = "button";
    pick.setAttribute("aria-pressed", "false");
    pick.dataset.pick = c[0];
    pick.addEventListener("click", function () { setPick(c[0]); });
    act.appendChild(pick);
    var sl = el("button", "btn", "Shortlist");
    sl.type = "button";
    sl.setAttribute("aria-pressed", "false");
    sl.addEventListener("click", function () {
      var k = shortlist.indexOf(c[0]);
      if (k === -1) shortlist.push(c[0]); else shortlist.splice(k, 1);
      sl.setAttribute("aria-pressed", k === -1 ? "true" : "false");
      li.classList.toggle("is-short", k === -1);
      update();
    });
    act.appendChild(sl);
    body.appendChild(act);
    li.appendChild(body);
    grid.appendChild(li);

    var tr = el("tr");
    tr.appendChild(el("td", null, num));
    var th = el("th"); th.scope = "row";
    var a = el("a", null, c[1]); a.href = href(c); th.appendChild(a);
    tr.appendChild(th);
    tr.appendChild(el("td", null, c[4]));
    tr.appendChild(el("td", null, c[5]));
    tr.appendChild(el("td", null, c[6]));
    [c[7], c[8], c[9]].forEach(function (n) { var td = el("td"); td.appendChild(dots(n)); tr.appendChild(td); });
    cmp.appendChild(tr);
  });

  var recList = document.getElementById("rec-list");
  if (RECOMMEND.length === 0) {
    document.getElementById("recommendation").hidden = true;
  }
  RECOMMEND.forEach(function (r) {
    var c = CONCEPTS.filter(function (x) { return x[0] === r[0]; })[0];
    var li = el("li");
    var s = el("strong");
    var a = el("a", null, c[0].slice(0, 2) + " · " + c[1]);
    a.href = href(c);
    s.appendChild(a);
    li.appendChild(s);
    li.appendChild(el("span", null, r[1]));
    recList.appendChild(li);
  });

  function name(slug) {
    var c = CONCEPTS.filter(function (x) { return x[0] === slug; })[0];
    return c[0].slice(0, 2) + " " + c[1];
  }
  function setPick(slug) {
    picked = picked === slug ? null : slug;
    document.querySelectorAll("[data-pick]").forEach(function (b) {
      var on = b.dataset.pick === picked;
      b.setAttribute("aria-pressed", on ? "true" : "false");
      b.textContent = on ? "Picked" : "Pick";
    });
    update();
  }
  function update() {
    var parts = [];
    if (picked) parts.push("Picked: " + name(picked));
    if (shortlist.length) parts.push("Shortlist: " + shortlist.map(name).join(", "));
    document.getElementById("pick-summary").textContent = parts.length ? parts.join(" · ") : "Nothing picked yet.";
    document.getElementById("send").disabled = !picked && !shortlist.length;
  }

  document.getElementById("send").addEventListener("click", function () {
    var note = document.getElementById("note").value.trim();
    var summary = (picked ? "Picked " + name(picked) : "Shortlisted " + shortlist.map(name).join(", ")) + (note ? ": " + note : "");
    var data = { picked: picked, shortlist: shortlist.slice(), note: note };
    var status = document.getElementById("status");
    // Herdr UI injects window.herdr into local pages; elsewhere copy the result instead.
    if (window.herdr && typeof window.herdr.send === "function") {
      window.herdr.send(summary, data);
      status.textContent = "Added to notes. Press Send to agent.";
    } else if (navigator.clipboard) {
      navigator.clipboard.writeText(summary + "\n" + JSON.stringify(data)).then(function () {
        status.textContent = "Copied to clipboard.";
      }, function () { status.textContent = summary; });
    } else {
      status.textContent = summary;
    }
  });

  // Swap screenshot thumbnails for scaled live iframes on request.
  document.querySelectorAll(".g-view button").forEach(function (b) {
    b.addEventListener("click", function () {
      document.querySelectorAll(".g-view button").forEach(function (o) { o.setAttribute("aria-pressed", o === b ? "true" : "false"); });
      var live = b.dataset.view === "live";
      CONCEPTS.forEach(function (c) {
        var t = document.querySelector("#c" + c[0].slice(0, 2) + " .g-thumb");
        var f = t.querySelector("iframe");
        var img = t.querySelector("img");
        if (live && !f) {
          f = el("iframe");
          f.src = href(c);
          f.title = "Live preview of " + c[1];
          f.loading = "lazy";
          f.tabIndex = -1;
          f.setAttribute("aria-hidden", "true");
          t.insertBefore(f, img);
        }
        if (f) f.hidden = !live;
        img.hidden = live;
      });
    });
  });

  // A missing thumbnail falls back to a live preview of that concept.
  grid.querySelectorAll(".g-thumb img").forEach(function (img) {
    img.addEventListener("error", function () {
      var t = img.parentNode;
      if (t.querySelector("iframe")) return;
      var f = el("iframe");
      f.src = t.getAttribute("href");
      f.title = "Live preview";
      f.tabIndex = -1;
      f.setAttribute("aria-hidden", "true");
      t.insertBefore(f, img);
      img.hidden = true;
    });
  });
})();
