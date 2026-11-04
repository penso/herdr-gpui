document.addEventListener("DOMContentLoaded", function () {
  "use strict";
  var S = window.Sheprd;

  /* Every tablist on the page: the loop, the three moves, and Get Sheprd. */
  function tablist(list) {
    var tabs = Array.prototype.slice.call(list.querySelectorAll('[role="tab"]'));
    function select(tab, focus) {
      tabs.forEach(function (t) {
        var on = t === tab;
        t.setAttribute("aria-selected", on ? "true" : "false");
        t.tabIndex = on ? 0 : -1;
        document.getElementById(t.getAttribute("aria-controls")).hidden = !on;
      });
      if (focus) tab.focus();
    }
    tabs.forEach(function (t, i) {
      t.addEventListener("click", function () { select(t, false); });
      t.addEventListener("keydown", function (e) {
        var n = null;
        if (e.key === "ArrowRight") n = tabs[(i + 1) % tabs.length];
        if (e.key === "ArrowLeft") n = tabs[(i - 1 + tabs.length) % tabs.length];
        if (e.key === "Home") n = tabs[0];
        if (e.key === "End") n = tabs[tabs.length - 1];
        if (n) { e.preventDefault(); select(n, true); }
      });
    });
    return select;
  }
  var selects = Array.prototype.map.call(document.querySelectorAll('[role="tablist"]'), tablist);

  // Preselect the visitor's platform in Get Sheprd.
  var ua = navigator.userAgent;
  var guess = /Windows/.test(ua) ? "t-win" : /Linux|X11/.test(ua) && !/Android/.test(ua) ? "t-linux" : /iPhone|iPad|Android/.test(ua) ? "t-phone" : null;
  if (guess) selects[selects.length - 1](document.getElementById(guess), false);

  // Copy buttons on the install commands.
  var status = document.getElementById("copy-status");
  document.querySelectorAll(".get-panel pre").forEach(function (pre) {
    var b = document.createElement("button");
    b.type = "button";
    b.className = "copy";
    b.textContent = "Copy";
    b.addEventListener("click", function () {
      var text = pre.querySelector("code").textContent;
      if (!navigator.clipboard) { status.textContent = "Clipboard unavailable; select the command instead."; return; }
      navigator.clipboard.writeText(text).then(function () {
        b.textContent = "Copied";
        status.textContent = "Copied: " + text;
        setTimeout(function () { b.textContent = "Copy"; }, 1600);
      }, function () { status.textContent = "Copy failed; select the command instead."; });
    });
    pre.appendChild(b);
  });

  /* Feature views: one set of controls drives the table and the directory. */
  function el(tag, cls, text) {
    var n = document.createElement(tag);
    if (cls) n.className = cls;
    if (text != null) n.textContent = text;
    return n;
  }
  function features() {
    var body = document.getElementById("ft-body");
    var cards = document.getElementById("ft-cards");
    var tableWrap = document.getElementById("ft-table");
    var q = document.getElementById("ft-q");
    var area = document.getElementById("ft-area");
    var chips = document.getElementById("ft-chips");
    var count = document.getElementById("ft-count");
    var status = "all";
    var entries = [];
    var groups = [];

    S.AREAS.forEach(function (a, ai) {
      var o = el("option", null, a.name);
      o.value = a.id;
      area.appendChild(o);
      var group = el("section", "fc-group");
      group.appendChild(el("h3", null, a.name));
      var ul = el("ul", "fc-list");
      group.appendChild(ul);
      cards.appendChild(group);
      groups.push(group);
      S.FEATURES.filter(function (f) { return f.area === a.id; }).forEach(function (f, fi) {
        var id = "F" + String(ai + 1).padStart(2, "0") + "." + String(fi + 1).padStart(2, "0");
        var tr = el("tr");
        [id, a.name, f.name, f.desc].forEach(function (t) { tr.appendChild(el("td", null, t)); });
        var st = el("td");
        st.appendChild(S.badge(f.status));
        tr.appendChild(st);
        tr.appendChild(el("td", null, f.where || (f.area === "phone" ? "—" : "All desktop platforms")));
        body.appendChild(tr);

        var li = el("li", "fc-item");
        var top = el("div", "fc-top");
        top.appendChild(el("strong", null, f.name));
        top.appendChild(S.badge(f.status));
        li.appendChild(top);
        li.appendChild(el("p", null, f.desc));
        if (f.where) li.appendChild(el("p", "fc-where", f.where));
        ul.appendChild(li);

        entries.push({ f: f, tr: tr, li: li, text: (id + " " + f.name + " " + f.desc + " " + f.where + " " + a.name).toLowerCase() });
      });
    });

    function apply() {
      var text = q.value.trim().toLowerCase();
      var shown = 0;
      var last = null;
      entries.forEach(function (e) {
        var ok = (status === "all" || e.f.status === status) && (area.value === "all" || e.f.area === area.value) && (!text || e.text.indexOf(text) !== -1);
        e.tr.hidden = !ok;
        e.li.hidden = !ok;
        e.tr.classList.remove("area-start");
        if (ok) {
          if (last !== null && e.f.area !== last) e.tr.classList.add("area-start");
          last = e.f.area;
          shown++;
        }
      });
      groups.forEach(function (g) { g.hidden = !g.querySelector(".fc-item:not([hidden])"); });
      count.textContent = shown + " of " + entries.length + " features";
    }

    ["all"].concat(Object.keys(S.STATUS)).forEach(function (k) {
      var b = el("button", null, k === "all" ? "All" : S.STATUS[k].label);
      b.type = "button";
      b.setAttribute("aria-pressed", k === "all" ? "true" : "false");
      b.addEventListener("click", function () {
        status = k;
        chips.querySelectorAll("button").forEach(function (o) { o.setAttribute("aria-pressed", o === b ? "true" : "false"); });
        apply();
      });
      chips.appendChild(b);
    });
    q.addEventListener("input", apply);
    area.addEventListener("change", apply);

    var views = document.querySelectorAll(".ft-view button");
    function setView(v) {
      views.forEach(function (b) { b.setAttribute("aria-pressed", b.dataset.view === v ? "true" : "false"); });
      tableWrap.hidden = v !== "table";
      cards.hidden = v !== "cards";
    }
    views.forEach(function (b) { b.addEventListener("click", function () { setView(b.dataset.view); }); });
    // The table needs sideways scrolling on a phone, so phones start on the directory.
    setView(window.matchMedia("(max-width: 48rem)").matches ? "cards" : "table");
    apply();
  }
  if (S) features();

  /* Illustrative board: statuses loop. With reduced motion it stays still and says so. */
  var note = document.getElementById("motion-note");
  if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
    if (note) note.textContent = "Animation is off because your system prefers reduced motion.";
    return;
  }
  if (note) note.textContent = "Statuses change on a loop to show how the board updates.";
  var rows = Array.prototype.slice.call(document.querySelectorAll("#board-rows tr"));
  var log = document.getElementById("board-log");
  var next = { idle: "working", working: "done", blocked: "working", done: "idle" };
  var events = { working: "started a turn", done: "finished · review ready", blocked: "is asking a question", idle: "is idle" };
  var tick = 0;
  setInterval(function () {
    tick++;
    var i = (tick * 3) % rows.length;
    var s = tick % 5 === 0 ? "blocked" : next[rows[i].dataset.s];
    var c = rows[i].cells;
    rows[i].dataset.s = s;
    rows[i].querySelector(".sl").textContent = s;
    var li = el("li");
    li.appendChild(el("b", null, c[3].textContent));
    li.appendChild(document.createTextNode(" " + c[2].textContent + " " + events[s] + " · " + c[1].textContent));
    log.insertBefore(li, log.firstChild);
    while (log.children.length > 3) log.removeChild(log.lastChild);
  }, 2600);
});
