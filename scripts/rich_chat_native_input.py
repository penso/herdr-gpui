"""Exercise native pointer input only on the capture script's private Xvfb display."""

import ctypes as c
import hashlib
import json
import math
from pathlib import Path
import subprocess
import time

from rich_chat_resize import capture_resize


def last_record(log, prefix):
    # The app can be halfway through a write when this reader polls. Only a
    # newline-terminated record is complete; malformed complete records still
    # fail instead of silently hiding a fixture problem.
    for line in reversed(log.read_text(errors="replace").splitlines(keepends=True)):
        if line.endswith("\n") and line.startswith(prefix):
            return json.loads(line[len(prefix):])
    return None


def capture_native_input(env, directory, app, case):
    display_name = env["DISPLAY"]
    if not display_name.startswith(":") or not display_name[1:].isdigit():
        raise RuntimeError("Expected the private Xvfb display")
    directory = Path(directory)
    log = directory / "native.log"
    x = c.CDLL("libX11.so.6")
    test = c.CDLL("libXtst.so.6")
    pointer, window = c.c_void_p, c.c_ulong
    x.XOpenDisplay.argtypes = [c.c_char_p]
    x.XOpenDisplay.restype = pointer
    x.XDefaultRootWindow.argtypes = [pointer]
    x.XDefaultRootWindow.restype = window
    x.XQueryTree.argtypes = [pointer, window, c.POINTER(window), c.POINTER(window), c.POINTER(c.POINTER(window)), c.POINTER(c.c_uint)]
    x.XFetchName.argtypes = [pointer, window, c.POINTER(c.c_char_p)]
    x.XFree.argtypes = [pointer]
    x.XSetInputFocus.argtypes = [pointer, window, c.c_int, window]
    x.XTranslateCoordinates.argtypes = [pointer, window, window, c.c_int, c.c_int, c.POINTER(c.c_int), c.POINTER(c.c_int), c.POINTER(window)]
    x.XStringToKeysym.argtypes = [c.c_char_p]
    x.XStringToKeysym.restype = window
    x.XKeysymToKeycode.argtypes = [pointer, window]
    x.XKeysymToKeycode.restype = c.c_ubyte
    x.XSync.argtypes = [pointer, c.c_int]
    x.XCloseDisplay.argtypes = [pointer]
    test.XTestFakeKeyEvent.argtypes = [pointer, c.c_uint, c.c_int, window]
    test.XTestFakeButtonEvent.argtypes = [pointer, c.c_uint, c.c_int, window]
    test.XTestFakeMotionEvent.argtypes = [pointer, c.c_int, c.c_int, c.c_int, window]
    display = x.XOpenDisplay(display_name.encode())
    if not display:
        raise RuntimeError("Could not open the private selection display")
    button_down = False
    keys_down = []
    try:
        root = x.XDefaultRootWindow(display)
        root_return, parent = window(), window()
        children, count = c.POINTER(window)(), c.c_uint()
        if not x.XQueryTree(display, root, c.byref(root_return), c.byref(parent), c.byref(children), c.byref(count)):
            raise RuntimeError("Could not inspect private fixture windows")
        matches = []
        try:
            for i in range(count.value):
                title = c.c_char_p()
                if x.XFetchName(display, children[i], c.byref(title)) and title.value:
                    try:
                        if title.value.decode(errors="replace") == "Herdr · Rich chat preview":
                            matches.append(children[i])
                    finally:
                        x.XFree(c.cast(title, pointer))
        finally:
            x.XFree(c.cast(children, pointer))
        if len(matches) != 1:
            raise RuntimeError(f"Expected one private fixture, found {len(matches)}")
        x.XSetInputFocus(display, matches[0], 2, 0)
        origin_x, origin_y, child = c.c_int(), c.c_int(), window()
        if not x.XTranslateCoordinates(display, matches[0], root, 0, 0, c.byref(origin_x), c.byref(origin_y), c.byref(child)):
            raise RuntimeError("Could not translate fixture coordinates")
        x.XSync(display, False)

        def record(prefix):
            return last_record(log, prefix)

        def wait_state(expected, copied):
            deadline = time.monotonic() + 4
            while time.monotonic() < deadline:
                state = record("RICH CHAT SELECTION STATE ")
                if state and state["selected"] == expected and state["clipboard"] == copied:
                    if expected and not state["reader_focused"]:
                        raise AssertionError("Selection did not own keyboard focus")
                    return state
                if app.poll() is not None:
                    raise RuntimeError("Selection fixture exited during input")
                time.sleep(0.05)
            raise TimeoutError(f"Native selection mismatch: expected={expected!r}, observed={state!r}")

        def chord(names):
            for name in names:
                code = x.XKeysymToKeycode(display, x.XStringToKeysym(name.encode()))
                if not code:
                    raise RuntimeError(f"Missing test key: {name}")
                test.XTestFakeKeyEvent(display, code, True, 0)
                keys_down.append(code)
            for code in reversed(keys_down):
                test.XTestFakeKeyEvent(display, code, False, 0)
            keys_down.clear()
            x.XSync(display, False)

        def motion(point):
            # Carets at the exclusive right edge are nudged inside by half a
            # pixel; flooring keeps XTest's integer coordinate inside too.
            test.XTestFakeMotionEvent(display, -1, math.floor(point[0]) + origin_x.value, math.floor(point[1]) + origin_y.value, 0)
            x.XSync(display, False)

        def button(down):
            nonlocal button_down
            test.XTestFakeButtonEvent(display, 1, down, 0)
            button_down = down
            x.XSync(display, False)

        def drag(start, end):
            motion(start)
            button(True)
            time.sleep(0.08)
            for step in range(1, 7):
                motion([start[i] + (end[i] - start[i]) * step / 6 for i in range(2)])
                time.sleep(0.03)
            button(False)

        def double_click(point):
            motion(point)
            for _ in range(2):
                button(True)
                button(False)
                time.sleep(0.08)

        def capture_stable_frame(output, area=None):
            # Selection state is reported before its repaint. Wait for stable
            # pixels so a screenshot cannot retain an intermediate drag frame.
            deadline = time.monotonic() + 4
            previous = None
            stable_since = None
            while time.monotonic() < deadline:
                capture = subprocess.run(
                    ["magick", "import", "-display", display_name, "-window", "root",
                     "ppm:-"],
                    env=env, capture_output=True, check=True, timeout=5,
                )
                # Ignore animated sidebar/status chrome. Compare actual
                # transcript pixels, without PNG timestamps or metadata.
                kind, dimensions, maximum, pixels = capture.stdout.split(b"\n", 3)
                if kind != b"P6":
                    raise RuntimeError("Native screenshot did not return RGB pixels")
                width, height = map(int, dimensions.split())
                stride = 3 * (2 if int(maximum) > 255 else 1)
                left, top, right, bottom = viewport if area is None else (
                    math.floor(area["x"]) + origin_x.value,
                    math.floor(area["y"]) + origin_y.value,
                    math.ceil(area["x"] + area["width"]) + origin_x.value,
                    math.ceil(area["y"] + area["height"]) + origin_y.value,
                )
                if not (0 <= left < right <= width and 0 <= top < bottom <= height):
                    raise RuntimeError("Transcript viewport is outside the native screenshot")
                region = b"".join(pixels[(row * width + left) * stride:
                                         (row * width + right) * stride]
                                  for row in range(top, bottom))
                current = hashlib.sha256(region).digest()
                now = time.monotonic()
                if current != previous:
                    previous, stable_since = current, now
                elif now - stable_since >= 0.2:
                    subprocess.run(["magick", "ppm:-", str(output)], input=capture.stdout,
                                   env=env, check=True, timeout=5)
                    return
                if app.poll() is not None:
                    raise RuntimeError("Selection fixture exited before a stable capture")
                time.sleep(0.05)
            raise TimeoutError("Native selection screenshot did not stabilize")

        if case == "input-resize":
            capture_resize(directory, app,
                           lambda: record("RICH CHAT INPUT SIZE "),
                           drag, motion, double_click, capture_stable_frame)
            return

        probe = record("RICH CHAT SELECTION PROBE ")
        if probe is None:
            raise RuntimeError("Missing native selection coordinates")
        geometry = record("RICH CHAT GEOMETRY ")
        if geometry is None:
            raise RuntimeError("Missing native transcript viewport")
        area = geometry["viewport"]
        viewport = (math.floor(area["x"]) + origin_x.value,
                    math.floor(area["y"]) + origin_y.value,
                    math.ceil(area["x"] + area["width"]) + origin_x.value,
                    math.ceil(area["y"] + area["height"]) + origin_y.value)
        cases = [
            ("forward", probe["from"], probe["to"], probe["expected"]),
            ("reverse", probe["to"], probe["from"], probe["expected"]),
            ("inline-math", probe["inline_from"], probe["inline_to"], probe["inline_expected"]),
            ("display-math", probe["math_from"], probe["math_to"], probe["math_expected"]),
            ("link-drag", probe["link_from"], probe["link_to"], probe["link_expected"]),
        ]
        results = []
        for name, start, end, expected in cases:
            chord(["Escape"])
            wait_state(None, "selection probe ready")
            drag(start, end)
            chord(["Control_L", "c"])
            state = wait_state(expected, expected)
            output = directory / f"selection-{name}.png"
            capture_stable_frame(output)
            results.append({"case": name, "state": state, "screenshot": output.name})
            (directory / "selection.json").write_text(json.dumps({"status": "passed" if len(results) == len(cases) else "running", "checks": results}, indent=2, ensure_ascii=False))
    finally:
        if button_down:
            test.XTestFakeButtonEvent(display, 1, False, 0)
        for code in reversed(keys_down):
            test.XTestFakeKeyEvent(display, code, False, 0)
        x.XSync(display, False)
        x.XCloseDisplay(display)
