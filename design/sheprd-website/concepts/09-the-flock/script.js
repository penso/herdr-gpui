document.addEventListener("DOMContentLoaded", function () {
  "use strict";
  var dir = document.getElementById("directory");
  if (dir && window.Sheprd) {
    dir.textContent = "";
    window.Sheprd.renderDirectory(dir, { collapsible: true, open: false, placeholder: "Search the pasture…" });
  }

  document.querySelectorAll(".copy").forEach(function (b) {
    b.addEventListener("click", function () {
      var text = b.getAttribute("data-copy");
      var done = function () { b.textContent = "Copied"; setTimeout(function () { b.textContent = "Copy"; }, 1600); };
      if (navigator.clipboard && navigator.clipboard.writeText) {
        navigator.clipboard.writeText(text).then(done, function () { b.textContent = "Select and copy"; });
      } else {
        b.textContent = "Select and copy";
      }
    });
  });
});
