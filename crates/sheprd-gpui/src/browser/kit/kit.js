// Sheprd explainer kit. Sheprd serves this file at /.herdr/kit.js to
// local pages; it needs kit.css and no other library. Everything it puts on
// the page goes in through textContent, so data is never parsed as HTML.
//
//   HerdrKit.scenario(root, spec)   controls, steps, detail and outcome
//   HerdrKit.segmented(options)     a row of mutually exclusive buttons
//   HerdrKit.variants(root, list)   mockup variants with Pick buttons
//   HerdrKit.send(summary, data)    hand the agent a result (herdr.send)
//   HerdrKit.rich(text)             `code` and **bold** in plain text
//   HerdrKit.h(tag, props, ...kids) build an element
(function () {
  "use strict";

  function h(tag, props) {
    var node = document.createElement(tag);
    var key;
    for (key in props || {}) {
      var value = props[key];
      if (value == null || value === false) continue;
      if (key === "class") node.className = value;
      else if (key === "text") node.textContent = value;
      else if (key.slice(0, 2) === "on" && typeof value === "function") {
        node.addEventListener(key.slice(2).toLowerCase(), value);
      } else node.setAttribute(key, value === true ? "" : String(value));
    }
    for (var i = 2; i < arguments.length; i++) append(node, arguments[i]);
    return node;
  }

  function append(node, child) {
    if (child == null || child === false) return;
    if (Array.isArray(child)) {
      child.forEach(function (each) { append(node, each); });
    } else if (child instanceof Node) {
      node.appendChild(child);
    } else {
      node.appendChild(document.createTextNode(String(child)));
    }
  }

  // Plain text with `code` spans and **bold** runs, as a fragment.
  function rich(text) {
    var fragment = document.createDocumentFragment();
    String(text == null ? "" : text)
      .split(/(`[^`]+`|\*\*[^*]+\*\*)/)
      .forEach(function (part) {
        if (!part) return;
        if (part[0] === "`") fragment.appendChild(h("code", { text: part.slice(1, -1) }));
        else if (part.slice(0, 2) === "**") fragment.appendChild(h("strong", { text: part.slice(2, -2) }));
        else fragment.appendChild(document.createTextNode(part));
      });
    return fragment;
  }

  function toast(text) {
    var node = document.querySelector(".hk-toast");
    if (!node) {
      node = h("div", { class: "hk-toast", role: "status" });
      document.body.appendChild(node);
    }
    node.textContent = text;
    node.setAttribute("data-shown", "");
    clearTimeout(toast.timer);
    toast.timer = setTimeout(function () { node.removeAttribute("data-shown"); }, 2200);
  }

  // Hands the agent a result through Sheprd, which queues it in the
  // page's notes for the user to send. Elsewhere it is copied instead.
  function send(summary, data) {
    if (window.herdr) {
      // Sheprd says itself whether it kept the result, in its notes panel
      // or a warning, so the page never claims a result the app refused.
      if (window.herdr.send(summary, data)) return true;
      toast("Not sent: a result needs a summary or data, at most " +
        Math.floor(window.herdr.maxDataBytes / 1024) + " KB of JSON");
      return false;
    }
    var text = summary + (data === undefined ? "" : "\n" + JSON.stringify(data, null, 2));
    if (navigator.clipboard) {
      navigator.clipboard.writeText(text).then(
        function () { toast("Not in Sheprd: copied the result"); },
        function () { toast("Not in Sheprd: " + summary); }
      );
    } else {
      toast("Not in Sheprd: " + summary);
    }
    return false;
  }

  // options: { options: [value | {value, label}], value, label, onChange }
  function segmented(options) {
    var group = h("div", { class: "hk-segmented", role: "group", "aria-label": options.label });
    var current = options.value;
    var buttons = (options.options || []).map(function (option) {
      var value = typeof option === "object" ? option.value : option;
      var label = typeof option === "object" ? option.label || option.value : option;
      var button = h("button", {
        type: "button",
        text: label,
        "aria-pressed": String(value === current),
        onClick: function () {
          if (value === current) return;
          current = value;
          buttons.forEach(function (each) {
            each.node.setAttribute("aria-pressed", String(each.value === value));
          });
          if (options.onChange) options.onChange(value);
        },
      });
      group.appendChild(button);
      return { node: button, value: value };
    });
    return group;
  }

  // The last step that ran: where a flow ended, or failed.
  function lastRan(list) {
    for (var i = list.length - 1; i >= 0; i--) {
      if (list[i].status !== "skip" && list[i].status !== "pending") return i;
    }
    return 0;
  }

  function firstValue(control) {
    var first = (control.options || [])[0];
    return typeof first === "object" ? first.value : first;
  }

  // spec: {
  //   title, lede,
  //   controls: { key: { label, options, value } },
  //   run(state) -> {
  //     steps: [{ label, status: "ok"|"fail"|"warn"|"skip"|"pending", detail }],
  //     select,                       the step shown first; else the last that ran
  //     outcome: { tone: "ok"|"warn"|"bad"|"info", title, body },
  //     asides: [text | { text, icon: "bell"|"info" }],
  //     callout: text,
  //     extra: Node,                  shown after the outcome
  //   }
  // }
  // Texts accept `code` and **bold**. Returns { state, render }.
  function scenario(root, spec) {
    var state = {};
    var controls = spec.controls || {};
    Object.keys(controls).forEach(function (key) {
      state[key] = controls[key].value !== undefined ? controls[key].value : firstValue(controls[key]);
    });
    var selected = null;
    root.textContent = "";
    root.classList.add("hk-scenario");
    if (spec.title) root.appendChild(h("p", { class: "hk-title" }, rich(spec.title)));
    if (spec.lede) root.appendChild(h("p", { class: "hk-lede" }, rich(spec.lede)));
    var controlRows = h("div", { class: "hk-controls" });
    Object.keys(controls).forEach(function (key) {
      var control = controls[key];
      var id = "hk-control-" + key;
      controlRows.appendChild(h("label", { id: id, text: control.label || key }));
      var row = segmented({
        options: control.options,
        value: state[key],
        label: control.label || key,
        onChange: function (value) {
          state[key] = value;
          selected = null;
          render();
        },
      });
      row.setAttribute("aria-labelledby", id);
      controlRows.appendChild(row);
    });
    if (Object.keys(controls).length) root.appendChild(controlRows);
    var steps = h("div", { class: "hk-steps", role: "tablist" });
    var detail = h("p", { class: "hk-detail", "aria-live": "polite" });
    var result = h("div");
    root.appendChild(steps);
    root.appendChild(detail);
    root.appendChild(result);

    function render() {
      var run = spec.run(Object.assign({}, state)) || {};
      var list = run.steps || [];
      if (selected == null || selected >= list.length) {
        selected = run.select != null ? run.select : lastRan(list);
      }
      // The steps are rebuilt, so keyboard focus on one moves to the
      // selected step's new button instead of being lost.
      var focused = steps.contains(document.activeElement);
      steps.textContent = "";
      list.forEach(function (step, index) {
        steps.appendChild(
          h(
            "button",
            {
              type: "button",
              class: "hk-step",
              role: "tab",
              "data-status": step.status || "pending",
              "aria-selected": String(index === selected),
              onClick: function () {
                selected = index;
                render();
              },
            },
            h("span", { class: "hk-step-head" }, h("span", { class: "hk-mark", "aria-hidden": "true" }), String(index + 1)),
            h("span", { class: "hk-step-label" }, rich(step.label))
          )
        );
      });
      if (focused && steps.children[selected]) steps.children[selected].focus();
      detail.textContent = "";
      if (list[selected]) detail.appendChild(rich(list[selected].detail || ""));
      result.textContent = "";
      if (list.length) result.appendChild(h("hr", { class: "hk-divider" }));
      if (run.outcome) {
        result.appendChild(
          h(
            "div",
            { class: "hk-outcome", "data-tone": run.outcome.tone || "info" },
            h("div", { class: "hk-outcome-title" }, rich(run.outcome.title)),
            run.outcome.body ? h("div", { class: "hk-outcome-body" }, rich(run.outcome.body)) : null
          )
        );
      }
      (run.asides || []).forEach(function (aside) {
        var text = typeof aside === "object" ? aside.text : aside;
        var icon = typeof aside === "object" ? aside.icon : null;
        result.appendChild(h("div", { class: "hk-aside", "data-icon": icon }, h("span", {}, rich(text))));
      });
      if (run.extra instanceof Node) result.appendChild(run.extra);
      if (run.callout) result.appendChild(h("p", { class: "hk-callout" }, rich(run.callout)));
    }

    render();
    return { state: state, render: render };
  }

  // list: [{ id, title, body: Node | text, note }]. Pick sends the agent
  // which variant the user chose; picking again changes the answer.
  function variants(root, list, options) {
    var settings = options || {};
    var cards = [];
    root.classList.add("hk-variants");
    root.textContent = "";
    list.forEach(function (variant) {
      var pick = h("button", {
        type: "button",
        class: "hk-button",
        text: "Pick",
        "aria-pressed": "false",
        onClick: function () {
          cards.forEach(function (card) {
            var chosen = card.id === variant.id;
            card.node.toggleAttribute("data-picked", chosen);
            card.pick.setAttribute("aria-pressed", String(chosen));
            card.pick.textContent = chosen ? "Picked" : "Pick";
          });
          send((settings.question ? settings.question + ": " : "Picked ") + variant.title, {
            picked: variant.id,
            title: variant.title,
          });
        },
      });
      var body = variant.body instanceof Node ? variant.body : rich(variant.body || "");
      var node = h(
        "section",
        { class: "hk-variant", "data-variant": variant.id },
        h("div", { class: "hk-variant-head" }, h("span", { text: variant.title }), pick),
        h("div", { class: "hk-variant-body" }, body, variant.note ? h("p", { class: "hk-lede" }, rich(variant.note)) : null)
      );
      cards.push({ id: variant.id, node: node, pick: pick });
      root.appendChild(node);
    });
  }

  window.HerdrKit = Object.freeze({
    h: h,
    rich: rich,
    toast: toast,
    send: send,
    segmented: segmented,
    scenario: scenario,
    variants: variants,
    inHerdr: Boolean(window.herdr && window.herdr.available),
  });
})();
