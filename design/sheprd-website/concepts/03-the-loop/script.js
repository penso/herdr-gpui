/* Concept 03 · The Loop: the step rail (WAI-ARIA tabs), copy buttons, directory. */
(function () {
  "use strict";

  function initRail() {
    var tabs = Array.prototype.slice.call(document.querySelectorAll('.rail [role="tab"]'));
    if (!tabs.length) return;

    function select(i, focus) {
      i = (i + tabs.length) % tabs.length;
      tabs.forEach(function (t, k) {
        var on = k === i;
        t.setAttribute("aria-selected", on ? "true" : "false");
        t.tabIndex = on ? 0 : -1;
        t.classList.toggle("done", k < i);
        document.getElementById(t.getAttribute("aria-controls")).hidden = !on;
      });
      if (focus) tabs[i].focus();
    }
    function current() {
      return tabs.findIndex(function (t) { return t.getAttribute("aria-selected") === "true"; });
    }

    tabs.forEach(function (t, i) {
      t.addEventListener("click", function () { select(i, false); });
      t.addEventListener("keydown", function (e) {
        var k = e.key, n = null;
        if (k === "ArrowRight" || k === "ArrowDown") n = i + 1;
        else if (k === "ArrowLeft" || k === "ArrowUp") n = i - 1;
        else if (k === "Home") n = 0;
        else if (k === "End") n = tabs.length - 1;
        if (n === null) return;
        e.preventDefault();
        select(n, true);
      });
    });

    document.querySelectorAll(".rail-nav [data-step]").forEach(function (b) {
      b.addEventListener("click", function () {
        select(current() + Number(b.dataset.step), false);
      });
    });
  }

  function initCopy() {
    document.querySelectorAll(".copy[data-copy]").forEach(function (b) {
      b.addEventListener("click", function () {
        var text = b.dataset.copy;
        var done = function () {
          b.textContent = "Copied";
          setTimeout(function () { b.textContent = "Copy"; }, 1600);
        };
        if (navigator.clipboard && navigator.clipboard.writeText) {
          navigator.clipboard.writeText(text).then(done, function () { b.textContent = "Select it"; });
        } else {
          b.textContent = "Select it";
        }
      });
    });
  }

  document.addEventListener("DOMContentLoaded", function () {
    initRail();
    initCopy();
    var dir = document.getElementById("directory");
    if (dir && window.Sheprd) window.Sheprd.renderDirectory(dir, { placeholder: "Search every feature…" });
  });
})();
