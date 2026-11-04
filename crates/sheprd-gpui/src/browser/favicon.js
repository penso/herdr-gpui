// Sheprd's favicon reader. The app evaluates this in a page's main frame
// each time the page loads. It reports the page's icon, drawn at `size`
// pixels square, as RGBA bytes, or null when the page has none the app can
// read, such as one from another origin that does not allow it. It watches
// the document's head so an icon the page swaps later is reported too, at
// most once per `pause` milliseconds. Each run, and each return of the page
// from the back-forward cache, reports again: the app's tab may show another
// page's icon by then. The app treats every report as untrusted.
(function (size, pause) {
  "use strict";
  if (window.__herdrFavicon) {
    window.__herdrFavicon(true);
    return;
  }
  var post = function (pixels) {
    try { window.ipc.postMessage(JSON.stringify({ kind: "favicon", pixels: pixels })); } catch (_) {}
  };

  // The declared icon closest to `size`, a scalable one first; failing
  // that, a web page's conventional `/favicon.ico`.
  var address = function () {
    var best = null;
    // Below every score, so a lone touch icon is still taken.
    var bestScore = -Infinity;
    var links = document.querySelectorAll("link[rel][href]");
    for (var i = 0; i < links.length; i++) {
      var link = links[i];
      var rel = (link.getAttribute("rel") || "").toLowerCase().split(/\s+/);
      var touch = rel.indexOf("apple-touch-icon") >= 0;
      if (rel.indexOf("icon") < 0 && !touch) continue;
      var sizes = (link.getAttribute("sizes") || "").toLowerCase();
      var score;
      if (sizes === "any" || /svg/.test(link.getAttribute("type") || "")) {
        score = 3000;
      } else {
        var side = parseInt(sizes, 10);
        // Too small blurs; too large only costs a bigger download.
        score = side > 0 ? (side >= size ? 2000 - side : 1000 + side) : 1500;
      }
      if (touch) score -= 2000;
      if (score > bestScore) {
        best = link.href;
        bestScore = score;
      }
    }
    if (best) return best;
    var web = location.protocol === "http:" || location.protocol === "https:";
    return web ? location.origin + "/favicon.ico" : null;
  };

  var shown;
  var draw = function (url) {
    var image = new Image();
    // Another origin's icon is readable only when it allows it.
    try {
      if (new URL(url).origin !== location.origin) image.crossOrigin = "anonymous";
    } catch (_) {}
    image.onload = function () {
      if (url !== shown) return;
      try {
        var canvas = document.createElement("canvas");
        canvas.width = canvas.height = size;
        var context = canvas.getContext("2d");
        var width = image.naturalWidth || size;
        var height = image.naturalHeight || size;
        var scale = Math.min(size / width, size / height);
        var w = width * scale;
        var h = height * scale;
        context.drawImage(image, (size - w) / 2, (size - h) / 2, w, h);
        post(Array.prototype.slice.call(context.getImageData(0, 0, size, size).data));
      } catch (_) {
        post(null);
      }
    };
    image.onerror = function () {
      if (url === shown) post(null);
    };
    image.src = url;
  };

  var check = function (again) {
    var url = address();
    if (url === shown && again !== true) return;
    shown = url;
    if (url === null) post(null);
    else draw(url);
  };

  var waiting = false;
  var later = function () {
    if (waiting) return;
    waiting = true;
    setTimeout(function () {
      waiting = false;
      check();
    }, pause);
  };
  window.__herdrFavicon = check;
  check();
  window.addEventListener("pageshow", function (event) {
    if (event.persisted) check(true);
  });
  if (document.head && window.MutationObserver) {
    new MutationObserver(later).observe(document.head, {
      childList: true,
      subtree: true,
      attributes: true,
      attributeFilter: ["href", "rel", "sizes", "type"],
    });
  }
})
