/* Concept 08: fill measurements from Sheprd.METRICS and render the dense feature table. */
(function () {
  "use strict";

  function fmt(v) {
    return typeof v === "number" ? v.toLocaleString("en-US") : String(v);
  }

  function metrics(S) {
    document.querySelectorAll("[data-metric]").forEach(function (n) {
      var v = S.METRICS[n.getAttribute("data-metric")];
      if (v != null) n.textContent = fmt(v);
    });
  }

  function table(S) {
    var body = document.getElementById("ft-body");
    var q = document.getElementById("ft-q");
    var st = document.getElementById("ft-status");
    var ar = document.getElementById("ft-area");
    var count = document.getElementById("ft-count");
    var areaName = {};
    var areaIndex = {};
    S.AREAS.forEach(function (a, i) {
      areaName[a.id] = a.name;
      areaIndex[a.id] = i + 1;
      var o = document.createElement("option");
      o.value = a.id;
      o.textContent = a.name;
      ar.appendChild(o);
    });

    var perArea = {};
    var rows = S.FEATURES.map(function (f) {
      perArea[f.area] = (perArea[f.area] || 0) + 1;
      var tr = document.createElement("tr");
      var id = "F" + String(areaIndex[f.area]).padStart(2, "0") + "." + String(perArea[f.area]).padStart(2, "0");
      [id, areaName[f.area], f.name, f.desc].forEach(function (t) {
        var td = document.createElement("td");
        td.textContent = t;
        tr.appendChild(td);
      });
      var tdS = document.createElement("td");
      tdS.appendChild(S.badge(f.status));
      tr.appendChild(tdS);
      var tdW = document.createElement("td");
      tdW.textContent = f.where || (f.area === "phone" ? "—" : "All desktop platforms");
      tr.appendChild(tdW);
      tr._f = f;
      tr._text = (id + " " + f.name + " " + f.desc + " " + f.where + " " + areaName[f.area]).toLowerCase();
      return tr;
    });
    body.textContent = "";
    rows.forEach(function (r) { body.appendChild(r); });

    function apply() {
      var text = q.value.trim().toLowerCase();
      var shown = 0;
      var lastArea = null;
      rows.forEach(function (r) {
        var ok = (st.value === "all" || r._f.status === st.value) &&
          (ar.value === "all" || r._f.area === ar.value) &&
          (!text || r._text.indexOf(text) !== -1);
        r.hidden = !ok;
        r.classList.remove("area-start");
        if (ok) {
          if (lastArea !== null && r._f.area !== lastArea) r.classList.add("area-start");
          lastArea = r._f.area;
          shown++;
        }
      });
      count.textContent = shown + " of " + rows.length + " rows";
    }
    q.addEventListener("input", apply);
    st.addEventListener("change", apply);
    ar.addEventListener("change", apply);
    apply();
  }

  document.addEventListener("DOMContentLoaded", function () {
    var S = window.Sheprd;
    if (!S) return;
    metrics(S);
    table(S);
  });
})();
