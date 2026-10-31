"""Click a real tool chevron only inside the capture script's private Xvfb."""

import json
import time


def capture_disclosure(directory, app, record, motion, button, capture):
    def wait_for(expanded):
        deadline = time.monotonic() + 4
        state = None
        while time.monotonic() < deadline:
            state = record()
            if state and state["expanded"] == expanded and state["source_visible"] == expanded:
                x, y = state["arrow"]
                viewport = state["viewport"]
                if not (viewport["x"] <= x < viewport["x"] + viewport["width"]
                        and viewport["y"] <= y < viewport["y"] + viewport["height"]):
                    raise AssertionError("Tool chevron is outside the rendered viewport")
                return state
            if app.poll() is not None:
                raise RuntimeError("Disclosure fixture exited during pointer checks")
            time.sleep(0.05)
        raise TimeoutError(f"Native disclosure did not reach expanded={expanded}: {state!r}")

    state = wait_for(False)
    results = []
    capture(directory / "tool-collapsed.png", state["viewport"])
    for name, expanded in [("expanded", True), ("collapsed-again", False), ("expanded-again", True)]:
        motion(state["arrow"])
        button(True)
        button(False)
        state = wait_for(expanded)
        capture(directory / f"tool-{name}.png", state["viewport"])
        # Layout/painter changes must not reverse an already observed toggle.
        state = wait_for(expanded)
        results.append({"case": name, "state": state})
    (directory / "tool-disclosure.json").write_text(json.dumps({
        "status": "passed", "checks": results,
    }, indent=2))
