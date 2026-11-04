document.addEventListener("DOMContentLoaded", function () {
  "use strict";
  var fd = document.getElementById("fd");
  if (fd && window.Sheprd) {
    window.Sheprd.renderDirectory(fd, { collapsible: true, open: false });
    var first = fd.querySelector("details");
    if (first) first.open = true;
  }

  var status = document.getElementById("copy-status");
  document.querySelectorAll(".copy").forEach(function (b) {
    b.setAttribute("aria-label", "Copy command: " + b.dataset.copy);
    b.addEventListener("click", function () {
      var say = function (label, msg) {
        b.textContent = label;
        if (status) status.textContent = msg;
        setTimeout(function () { b.textContent = "Copy"; }, 1600);
      };
      var ok = function () { say("Copied", "Copied: " + b.dataset.copy); };
      var fail = function () { say("Select it", "Copy is blocked here; select the command to copy it."); };
      if (navigator.clipboard && navigator.clipboard.writeText) {
        navigator.clipboard.writeText(b.dataset.copy).then(ok, fail);
      } else {
        fail();
      }
    });
  });
});
