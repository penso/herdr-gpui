document.addEventListener("DOMContentLoaded", function () {
  "use strict";
  if (!window.Sheprd) return;
  var n = window.Sheprd.FEATURES.length;
  document.querySelectorAll("[data-feature-count]").forEach(function (el) { el.textContent = n; });
  window.Sheprd.renderDirectory(document.getElementById("directory"), {
    collapsible: true,
    open: false,
    placeholder: "Search the index, e.g. teleport, Windows, phone…"
  });
});
