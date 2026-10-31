"""Exercise the native centered jump button on the private fixture display."""

import json
import time


def capture_jump(directory, app, record, motion, button, wheel, capture):
    def wait_for(predicate):
        deadline = time.monotonic() + 4
        state = None
        while time.monotonic() < deadline:
            state = record()
            if state and predicate(state):
                return state
            if app.poll() is not None:
                raise RuntimeError("Jump fixture exited during pointer checks")
            time.sleep(0.05)
        raise TimeoutError(f"Native jump did not reach expected state: {state!r}")

    initial = wait_for(lambda state: state["viewport"]["height"] > 0)
    viewport = initial["viewport"]
    motion([viewport["x"] + viewport["width"] / 2,
            viewport["y"] + viewport["height"] / 2])
    wheel(5, 30)
    wait_for(lambda state: state["at_latest"])
    wheel(4, 8)
    paused = wait_for(lambda state: state["needed"] and state["jump"] is not None)
    x, y = paused["jump"]
    if abs(x - (viewport["x"] + viewport["width"] / 2)) > 1:
        raise AssertionError("Jump control is not centered in its conversation")
    if not viewport["y"] <= y < viewport["y"] + viewport["height"]:
        raise AssertionError("Jump control is outside its conversation")
    capture(directory / "jump-centered.png", viewport)
    motion(paused["jump"])
    button(True)
    button(False)
    following = wait_for(lambda state: state["at_latest"] and not state["needed"])
    capture(directory / "jump-after.png", following["viewport"])
    (directory / "jump.json").write_text(json.dumps({
        "status": "passed", "paused": paused, "after": following,
    }, indent=2))
