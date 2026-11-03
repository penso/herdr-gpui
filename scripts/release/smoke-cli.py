#!/usr/bin/env python3
"""Run a built sheprd executable's windowless CLI paths, without Cargo.

The release jobs check the exact bytes they ship. `cargo test --release --test
cli` cannot: it rebuilds the binary with dev-dependency features (gpui's
test-support) and overwrites the one `cargo build` produced. CI's full CLI
suite (crates/sheprd-gpui/tests/cli.rs) covers the rest on the same commit.
"""
import argparse
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile

TIMEOUT = 10
# Variables a Herdr pane sets; with them gone no running app or daemon is reached.
PANE_VARIABLES = ("HERDR_SOCKET_PATH", "HERDR_CLIENT_SOCKET_PATH", "HERDR_WORKSPACE_ID", "HERDR_PANE_ID")
HELP_OPTIONS = ("--socket CLIENT_SOCKET", "--session NAME", "--dev", "--build-info")
BUILD_INFO = re.compile(r"worktree=[^\n]*\nbranch=[^\n]*\npr=[^\n]*\n")


class SmokeError(Exception):
    pass


def run(executable, args, state):
    env = {name: value for name, value in os.environ.items() if name not in PANE_VARIABLES}
    env["XDG_STATE_HOME"] = str(state)
    try:
        return subprocess.run([str(executable), *args], stdin=subprocess.DEVNULL, capture_output=True,
                              env=env, timeout=TIMEOUT, check=False)
    except subprocess.TimeoutExpired as error:
        raise SmokeError(f"{args}: did not exit within {TIMEOUT}s") from error


def text(output):
    return output.decode("utf-8", errors="replace")


def check_help(executable, state):
    for flag in ("--help", "-h"):
        result = run(executable, [flag], state)
        if result.returncode != 0 or result.stderr:
            raise SmokeError(f"{flag}: exit {result.returncode}: {text(result.stderr)}")
        help_text = text(result.stdout)
        for option in HELP_OPTIONS:
            if option not in help_text:
                raise SmokeError(f"{flag}: missing {option}")
        # The native test hooks are a separate feature that never ships.
        if "--integration-test" in help_text:
            raise SmokeError(f"{flag}: release build documents --integration-test")


def check_build_info(executable, state):
    result = run(executable, ["--build-info"], state)
    if result.returncode != 0 or result.stderr or not BUILD_INFO.fullmatch(text(result.stdout)):
        raise SmokeError(f"--build-info: exit {result.returncode}: {text(result.stdout)}{text(result.stderr)}")


def check_usage_error(executable, state, args, message):
    result = run(executable, args, state)
    stderr = text(result.stderr)
    # An abort or a window failing to open is not a parser rejection.
    if result.returncode != 2 or result.stdout or message not in stderr or "Usage: sheprd" not in stderr:
        raise SmokeError(f"{args}: exit {result.returncode}, expected usage error {message!r}: {stderr}")


def smoke(executable):
    with tempfile.TemporaryDirectory(prefix="herdr-cli-") as state:
        check_help(executable, state)
        check_build_info(executable, state)
        check_usage_error(executable, state, ["--unknown"], "Unknown option: --unknown")
        check_usage_error(executable, state, ["--socket"], "--socket requires a path")
        check_usage_error(executable, state, ["--integration-test"], "Unknown option: --integration-test")
        check_usage_error(executable, state, ["browser", "open"], "browser open requires one URL")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("executable", type=Path)
    args = parser.parse_args(argv)
    if not args.executable.is_file():
        parser.error(f"not a file: {args.executable}")
    try:
        smoke(args.executable.resolve())
    except SmokeError as error:
        print(f"CLI smoke test failed: {error}", file=sys.stderr)
        return 1
    print(f"CLI smoke test passed: {args.executable}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
