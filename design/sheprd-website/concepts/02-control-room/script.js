document.addEventListener("DOMContentLoaded", function () {
  "use strict";
  var S = window.Sheprd;
  if (S) {
    document.querySelectorAll("[data-feature-count]").forEach(function (el) { el.textContent = S.FEATURES.length; });
    S.renderDirectory(document.getElementById("directory"), { placeholder: "grep features: teleport, windows, phone…" });
  }

  // Copy buttons for install commands.
  var status = document.getElementById("copy-status");
  document.querySelectorAll(".cmd .copy").forEach(function (btn) {
    btn.addEventListener("click", function () {
      var text = btn.parentNode.querySelector("code").textContent;
      var done = function () {
        btn.textContent = "Copied";
        status.textContent = "Copied: " + text;
        setTimeout(function () { btn.textContent = "Copy"; }, 1600);
      };
      if (navigator.clipboard && navigator.clipboard.writeText) {
        navigator.clipboard.writeText(text).then(done, function () { status.textContent = "Copy failed; select the command instead."; });
      } else {
        status.textContent = "Clipboard unavailable; select the command instead.";
      }
    });
  });

  // Illustrative board: statuses change over time. Motion is skipped entirely
  // when the visitor prefers reduced motion; the static board still reads.
  var note = document.getElementById("motion-note");
  var reduce = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  if (reduce) {
    if (note) note.textContent = "Animation is off because your system prefers reduced motion.";
    return;
  }
  if (note) note.textContent = "Statuses change on a loop to show how the board updates.";

  var rows = Array.prototype.slice.call(document.querySelectorAll("#board-rows tr"));
  var log = document.getElementById("board-log");
  var next = { idle: "working", working: "done", blocked: "working", done: "idle" };
  var events = {
    working: "started a turn",
    done: "finished · review ready",
    blocked: "is asking a question",
    idle: "is idle"
  };
  var secs = rows.map(function (r) {
    var t = r.querySelector(".t").textContent.split(":");
    return t.length === 2 ? (+t[0]) * 60 + (+t[1]) : 0;
  });

  function setStatus(i, s) {
    var r = rows[i];
    r.dataset.s = s;
    r.querySelector(".sl").textContent = s;
    if (s === "working" || s === "blocked") secs[i] = s === "working" ? 0 : secs[i];
    var c = r.cells;
    var li = document.createElement("li");
    var b = document.createElement("b");
    b.textContent = c[3].textContent;
    li.appendChild(b);
    li.appendChild(document.createTextNode(" " + c[2].textContent + " " + events[s] + " · " + c[1].textContent));
    log.insertBefore(li, log.firstChild);
    while (log.children.length > 3) log.removeChild(log.lastChild);
  }

  setInterval(function () {
    rows.forEach(function (r, i) {
      if (r.dataset.s === "working") {
        secs[i] += 1;
        var m = Math.floor(secs[i] / 60), s = secs[i] % 60;
        r.querySelector(".t").textContent = (m < 10 ? "0" : "") + m + ":" + (s < 10 ? "0" : "") + s;
      }
    });
  }, 1000);

  var tick = 0;
  setInterval(function () {
    tick++;
    var i = (tick * 3) % rows.length;
    var cur = rows[i].dataset.s;
    var s = tick % 5 === 0 ? "blocked" : next[cur];
    setStatus(i, s);
  }, 2600);
});
