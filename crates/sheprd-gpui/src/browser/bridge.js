// Sheprd's page bridge, run before a local page's own scripts. A local
// page is one the agent wrote, so it may hand the agent a result, such as
// the variant the user picked in a mockup:
//
//   herdr.send("Picked variant B", { variant: "B" });
//
// The app only queues it as a note in the page's notes panel: nothing
// reaches the agent until the user presses Send there.
(function () {
  "use strict";
  // The script stays with the view wherever it navigates; only the local
  // page's own scheme gets the bridge.
  var local = /^(herdr-preview:|https?:\/\/herdr-preview\.localhost[/:])/;
  if (!local.test(String(location.href)) || window.herdr) {
    return;
  }
  if (!window.ipc || typeof window.ipc.postMessage !== "function") {
    return;
  }
  var post = window.ipc.postMessage.bind(window.ipc);
  // The app refuses a result whose data, as compact JSON, is larger than
  // this, or one with neither a summary nor data. Checking here lets send
  // say so instead of reporting a result that was never kept.
  var MAX_DATA_BYTES = 8192;
  function bytes(text) {
    return new TextEncoder().encode(text).length;
  }
  Object.defineProperty(window, "herdr", {
    value: Object.freeze({
      available: true,
      maxDataBytes: MAX_DATA_BYTES,
      // True when the app was handed the result, which it then confirms or
      // refuses in its own window; false when it would surely be refused:
      // no summary and no data, data over the limit, or data JSON cannot
      // represent.
      send: function (summary, data) {
        var message = { kind: "send", summary: String(summary == null ? "" : summary) };
        try {
          if (data !== undefined && data !== null) {
            var json = JSON.stringify(data);
            if (json === undefined || bytes(json) > MAX_DATA_BYTES) {
              return false;
            }
            message.data = data;
          } else if (!message.summary.replace(/[\s\u0000-\u001f\u007f-\u009f]/g, "")) {
            // The app reads a summary of only spaces and controls as empty.
            return false;
          }
          post(JSON.stringify(message));
          return true;
        } catch (_) {
          return false;
        }
      },
    }),
  });
})();
