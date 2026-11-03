/* Concept 07: accessible tabs, copy buttons and the feature directory. */
(function () {
  "use strict";

  function tabs(list) {
    var buttons = Array.prototype.slice.call(list.querySelectorAll('[role="tab"]'));
    function select(btn, focus) {
      buttons.forEach(function (b) {
        var on = b === btn;
        b.setAttribute("aria-selected", on ? "true" : "false");
        b.tabIndex = on ? 0 : -1;
        var panel = document.getElementById(b.getAttribute("aria-controls"));
        if (panel) panel.hidden = !on;
      });
      if (focus) btn.focus();
    }
    buttons.forEach(function (b, i) {
      b.addEventListener("click", function () { select(b, false); });
      b.addEventListener("keydown", function (e) {
        var next = null;
        if (e.key === "ArrowRight") next = buttons[(i + 1) % buttons.length];
        if (e.key === "ArrowLeft") next = buttons[(i - 1 + buttons.length) % buttons.length];
        if (e.key === "Home") next = buttons[0];
        if (e.key === "End") next = buttons[buttons.length - 1];
        if (next) { e.preventDefault(); select(next, true); }
      });
    });
  }

  function copyButtons() {
    document.querySelectorAll(".copy").forEach(function (b) {
      b.addEventListener("click", function () {
        var text = b.getAttribute("data-copy");
        var done = function () {
          b.textContent = "Copied";
          setTimeout(function () { b.textContent = "Copy"; }, 1600);
        };
        if (navigator.clipboard && navigator.clipboard.writeText) {
          navigator.clipboard.writeText(text).then(done, function () { b.textContent = "Select and copy"; });
        } else {
          b.textContent = "Select and copy";
        }
      });
    });
  }

  document.addEventListener("DOMContentLoaded", function () {
    document.querySelectorAll('[role="tablist"]').forEach(tabs);
    copyButtons();
    var dir = document.getElementById("directory");
    if (dir && window.Sheprd) {
      window.Sheprd.renderDirectory(dir, { collapsible: true, open: false });
      var first = dir.querySelector(".fd-group");
      if (first) first.open = true;
    }
  });
})();
