#!/usr/bin/env python3
"""Capture the real native chat fixture on a private X server, never the desktop.

Requires a build with `--features integration-test`, Xvfb, and ImageMagick.
The selection and input-resize cases use libX11 and libXtst for isolated native input.
Pass --xvfb and --vulkan-library for project-local test tools. No browser or
daemon is used. Run under a CPU/memory cap on shared workstations.
"""

import argparse
import json
import os
from pathlib import Path
import select
import subprocess
import tempfile
import time

from rich_chat_native_input import capture_native_input


def stop(process):
    """Only stop children created and retained by this invocation."""
    if process is None or process.poll() is not None:
        return
    process.terminate()
    try:
        process.wait(timeout=5)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait(timeout=5)


def capture(args, case, theme, width, root):
    name = f"{case}-{theme}-{width}"
    if args.layout:
        name += f"-{args.layout}"
    if args.max_width is not None:
        name += f"-width{args.max_width}"
    directory = root / name
    directory.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="herdr-native-") as temporary:
        sandbox = Path(temporary)
        env = {k: v for k, v in os.environ.items() if not k.startswith("HERDR_")}
        env.pop("WAYLAND_DISPLAY", None)
        for key in list(env):
            if key.startswith("DBUS_"):
                del env[key]
        env.pop("SSH_AUTH_SOCK", None)
        env["HOME"] = str(sandbox)
        # GPUI's Linux opener tries these PATH commands before its portal.
        # A failed link-drag test must be observable without launching a browser.
        opener_bin = sandbox / "openers"
        opener_bin.mkdir()
        opener_log = directory / "unexpected-open.log"
        env["HERDR_TEST_OPEN_LOG"] = str(opener_log)
        for opener in ("xdg-open", "gio", "gnome-open", "kde-open", "sensible-browser"):
            stub = opener_bin / opener
            stub.write_text('#!/bin/sh\nprintf "%s\\n" "$0 $*" >> "$HERDR_TEST_OPEN_LOG"\nexit 0\n')
            stub.chmod(0o700)
        env["PATH"] = str(opener_bin) + os.pathsep + env.get("PATH", "")
        env["BROWSER"] = str(opener_bin / "xdg-open")
        for key in ["XDG_CONFIG_HOME", "XDG_STATE_HOME", "XDG_CACHE_HOME", "XDG_DATA_HOME", "XDG_RUNTIME_DIR"]:
            path = sandbox / key.lower()
            path.mkdir(mode=0o700)
            env[key] = str(path)
        env.update({
            "HERDR_TEST_RICH_CHAT": case,
            "HERDR_TEST_RICH_CHAT_THEME": theme,
            "HERDR_TEST_RICH_CHAT_SIZE": width,
            "HERDR_TEST_RICH_CHAT_HOLD_MS": "15000" if case in ("selection", "input-resize") else "3000",
            "HERDR_CONFIG_PATH": str(sandbox / "daemon.toml"),
            "LP_NUM_THREADS": "2",
            "RAYON_NUM_THREADS": "2",
        })
        if args.max_width is not None:
            env["HERDR_TEST_RICH_CHAT_MAX_WIDTH"] = str(args.max_width)
        if args.read_only:
            env["HERDR_TEST_RICH_CHAT_READ_ONLY"] = "1"
        if args.layout:
            env["HERDR_TEST_RICH_CHAT_LAYOUT"] = args.layout
        if args.vulkan_library:
            icd = sandbox / "vulkan.json"
            icd.write_text(json.dumps({"file_format_version": "1.0.1", "ICD": {
                "api_version": "1.4.0", "library_path": str(args.vulkan_library.resolve())
            }}))
            env["VK_DRIVER_FILES"] = str(icd)
        reader, writer = os.pipe()
        server = app = None
        try:
            with (directory / "xvfb.log").open("w") as server_log:
                server = subprocess.Popen([
                    str(args.xvfb), "-displayfd", str(writer), "-screen", "0", "1600x1000x24",
                    "-nolisten", "tcp", "-ac", "-noreset",
                ], pass_fds=(writer,), stdout=server_log, stderr=subprocess.STDOUT)
                os.close(writer)
                writer = None
                if not select.select([reader], [], [], 10)[0]:
                    raise RuntimeError("private X server did not become ready")
                number = os.read(reader, 32).decode().strip()
                if not number.isdigit():
                    raise RuntimeError("private X server returned an invalid display")
                env["DISPLAY"] = f":{number}"
                log_path = directory / "native.log"
                with log_path.open("w") as app_log:
                    app = subprocess.Popen([str(args.binary.resolve()), "--sidebar-test"], env=env,
                                           stdout=app_log, stderr=subprocess.STDOUT)
                    deadline = time.monotonic() + 55
                    while "RICH CHAT READY" not in log_path.read_text(errors="replace"):
                        if app.poll() is not None:
                            raise RuntimeError(f"fixture exited {app.returncode}: {log_path.read_text()[-4000:]}")
                        if time.monotonic() >= deadline:
                            raise TimeoutError(f"fixture did not settle: {log_path}")
                        time.sleep(0.1)
                    if case in ("selection", "input-resize"):
                        capture_native_input(env, directory, app, case)
                    output = directory / "native.png"
                    subprocess.run(["magick", "import", "-display", env["DISPLAY"], "-window", "root",
                                    str(output)], env=env, check=True, timeout=10)
                    status = app.wait(timeout=20 if case in ("selection", "input-resize") else 10)
                    if status or "native fixture PASS" not in log_path.read_text(errors="replace"):
                        raise RuntimeError(f"fixture did not pass: {log_path}")
                    if opener_log.exists():
                        raise AssertionError(f"Native fixture unexpectedly opened a link: {opener_log}")
                    print(f"Captured {name}: {output}", flush=True)
        finally:
            stop(app)
            stop(server)
            os.close(reader)
            if writer is not None:
                os.close(writer)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("target/debug/herdr-gpui"))
    parser.add_argument("--xvfb", type=Path, default=Path("/usr/bin/Xvfb"))
    parser.add_argument("--vulkan-library", type=Path)
    parser.add_argument("--output", type=Path, default=Path("target/rich-chat-native"))
    parser.add_argument("--case", choices=["math", "code", "diagram", "activity", "activity-open", "attachments", "questions", "selection", "input-resize", "user", "widgets", "widgets-end", "conversation", "existing", "all"])
    parser.add_argument("--layout", choices=["timeline", "bubbles", "terminal", "document", "cards", "imessage"])
    parser.add_argument("--theme", choices=["dark", "light"], default="dark")
    parser.add_argument("--size", choices=["wide", "narrow"], default="wide")
    parser.add_argument("--read-only", action="store_true")
    parser.add_argument("--max-width", type=int, help="Reading column in logical pixels: 0 for full width, or 320..4000")
    args = parser.parse_args()
    if args.max_width is not None and args.max_width != 0 and not 320 <= args.max_width <= 4000:
        parser.error("--max-width expects 0 or 320..4000")
    cases = [(args.case, args.theme, args.size)] if args.case else [
        ("math", "dark", "wide"), ("math", "light", "wide"), ("math", "dark", "narrow"),
        ("code", "dark", "wide"), ("code", "light", "wide"), ("code", "dark", "narrow"),
        ("diagram", "dark", "wide"), ("widgets", "dark", "wide"), ("widgets-end", "light", "narrow"),
        ("conversation", "dark", "wide"), ("conversation", "light", "wide"), ("conversation", "dark", "narrow"),
        ("existing", "dark", "wide"), ("existing", "light", "narrow"),
        ("input-resize", "dark", "wide"), ("input-resize", "light", "narrow"),
        ("attachments", "dark", "narrow"),
        ("questions", "dark", "wide"), ("questions", "light", "narrow"),
        ("selection", "dark", "narrow"), ("selection", "light", "narrow"),
    ]
    for case, theme, width in cases:
        capture(args, case, theme, width, args.output.resolve())


if __name__ == "__main__":
    main()
