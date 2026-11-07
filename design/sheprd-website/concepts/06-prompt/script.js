document.addEventListener("DOMContentLoaded", function () {
  "use strict";
  var S = window.Sheprd;

  /* ls -l style listing built from the shared dataset. */
  var ls = document.getElementById("ls");
  var q = document.getElementById("q");
  var total = document.getElementById("total");
  var flags = document.querySelectorAll(".flags button");
  var active = "all";
  var LABEL = { shipped: "shipped", optin: "opt-in", experimental: "experimental", testflight: "testflight", planned: "planned" };
  var rows = [];

  function span(cls, text) {
    var s = document.createElement("span");
    s.className = cls;
    s.textContent = text;
    return s;
  }

  if (S && ls) {
    S.AREAS.forEach(function (a) {
      var head = document.createElement("p");
      head.className = "ls-area";
      head.textContent = a.id + "/  # " + a.name;
      head.setAttribute("role", "heading");
      head.setAttribute("aria-level", "3");
      ls.appendChild(head);
      var group = { head: head, items: [] };
      S.FEATURES.filter(function (f) { return f.area === a.id; }).forEach(function (f) {
        var row = document.createElement("div");
        row.className = "ls-row";
        var perm = f.status === "planned" ? "----------" : f.status === "shipped" ? "-rwxr-xr-x" : "-rw-r--r--";
        var p = span("ls-perm", perm);
        p.setAttribute("aria-hidden", "true");
        row.appendChild(p);
        row.appendChild(span("ls-st s-" + f.status, LABEL[f.status]));
        row.appendChild(span("ls-name", f.name));
        row.appendChild(span("ls-desc", f.desc));
        if (f.where) row.appendChild(span("ls-where", "! " + f.where));
        row._text = (f.name + " " + f.desc + " " + f.where + " " + a.name + " " + a.id).toLowerCase();
        row._status = f.status;
        ls.appendChild(row);
        group.items.push(row);
        rows.push(row);
      });
      group.head._group = group;
      ls._groups = (ls._groups || []).concat(group);
    });
  }

  function apply() {
    var term = q ? q.value.trim().toLowerCase() : "";
    var shown = 0;
    rows.forEach(function (r) {
      var ok = (active === "all" || r._status === active) && (!term || r._text.indexOf(term) !== -1);
      r.hidden = !ok;
      if (ok) shown++;
    });
    (ls && ls._groups || []).forEach(function (g) {
      g.head.hidden = !g.items.some(function (r) { return !r.hidden; });
    });
    if (total) total.textContent = "total " + shown + " of " + rows.length;
  }

  flags.forEach(function (b) {
    b.addEventListener("click", function () {
      active = b.dataset.status;
      flags.forEach(function (o) { o.setAttribute("aria-pressed", o === b ? "true" : "false"); });
      apply();
    });
  });
  if (q) q.addEventListener("input", apply);
  apply();

  /* "/" focuses the grep, like a pager search. */
  document.addEventListener("keydown", function (e) {
    var t = e.target;
    var typing = t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.isContentEditable);
    if (e.key === "/" && !typing && q) {
      e.preventDefault();
      q.focus();
      q.scrollIntoView({ block: "center" });
    }
  });

  /* Copy buttons with an announced result. */
  var status = document.getElementById("copy-status");
  document.querySelectorAll(".copy").forEach(function (b) {
    b.setAttribute("aria-label", "Copy command: " + b.dataset.copy);
    b.addEventListener("click", function () {
      var say = function (label, msg) {
        b.textContent = label;
        if (status) status.textContent = msg;
        setTimeout(function () { b.textContent = "copy"; }, 1600);
      };
      if (navigator.clipboard && navigator.clipboard.writeText) {
        navigator.clipboard.writeText(b.dataset.copy).then(
          function () { say("copied", "Copied: " + b.dataset.copy); },
          function () { say("select", "Copy is blocked here; select the command to copy it."); }
        );
      } else {
        say("select", "Copy is blocked here; select the command to copy it.");
      }
    });
  });

  /* Highlight the tmux window for the section in view. */
  var links = Array.prototype.slice.call(document.querySelectorAll(".tmux a"));
  if ("IntersectionObserver" in window) {
    var io = new IntersectionObserver(function (entries) {
      entries.forEach(function (en) {
        if (!en.isIntersecting) return;
        links.forEach(function (l) {
          var on = l.getAttribute("href") === "#" + en.target.id;
          if (on) l.setAttribute("aria-current", "true"); else l.removeAttribute("aria-current");
        });
      });
    }, { rootMargin: "-40% 0px -55% 0px" });
    links.forEach(function (l) {
      var sec = document.querySelector(l.getAttribute("href"));
      if (sec) io.observe(sec);
    });
  }
});
