document.addEventListener("DOMContentLoaded", function () {
  "use strict";
  var root = document.documentElement;

  // Feature directory.
  var dir = document.getElementById("directory");
  if (dir && window.Sheprd) {
    dir.textContent = "";
    window.Sheprd.renderDirectory(dir, { placeholder: "Search every feature…" });
  }

  // Opening screenshot flattens once scrolled into view.
  var shot = document.querySelector(".opening-shot");
  if (shot && "IntersectionObserver" in window) {
    new IntersectionObserver(function (es) {
      es.forEach(function (e) { if (e.intersectionRatio > 0.35) shot.classList.add("flat"); });
    }, { threshold: [0, 0.35] }).observe(shot);
  }

  // Scrollytelling: only on wide screens without reduced motion; otherwise the
  // chapters keep their inline screenshots.
  var wide = window.matchMedia("(min-width: 1000px)");
  var still = window.matchMedia("(prefers-reduced-motion: reduce)");
  var chapters = Array.prototype.slice.call(document.querySelectorAll(".chapter"));
  var views = document.querySelectorAll(".frame-view > [data-key]");
  var dots = document.querySelectorAll(".progress li");
  var observer = null;

  function show(key) {
    chapters.forEach(function (c) { c.classList.toggle("on", c.dataset.key === key); });
    views.forEach(function (v) { v.classList.toggle("on", v.dataset.key === key); });
    dots.forEach(function (d) { d.classList.toggle("on", d.dataset.key === key); });
  }

  function setup() {
    var on = wide.matches && !still.matches && "IntersectionObserver" in window;
    root.classList.toggle("scrolly", on);
    if (observer) { observer.disconnect(); observer = null; }
    if (!on) return;
    observer = new IntersectionObserver(function (entries) {
      entries.forEach(function (e) { if (e.isIntersecting) show(e.target.dataset.key); });
    }, { rootMargin: "-45% 0px -45% 0px" });
    chapters.forEach(function (c) { observer.observe(c); });
    show(chapters[0].dataset.key);
  }
  setup();
  [wide, still].forEach(function (m) {
    if (m.addEventListener) m.addEventListener("change", setup); else m.addListener(setup);
  });

  // Install tabs (ARIA tabs pattern with arrow keys).
  var tabs = Array.prototype.slice.call(document.querySelectorAll('.tabs [role="tab"]'));
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
  // Preselect the visitor's platform.
  var ua = navigator.userAgent;
  var guess = /Windows/.test(ua) ? "t-win" : /Linux|X11/.test(ua) && !/Android/.test(ua) ? "t-linux" : /iPhone|iPad|Android/.test(ua) ? "t-phone" : null;
  if (guess) select(document.getElementById(guess), false);
});
