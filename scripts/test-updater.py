#!/usr/bin/env python3
"""Test the real updater without compiling GPUI (Python 3.11+, Rust via rustup).

The updater is its own crate, `sheprd-updater`, so this builds and tests only
that crate and its dependencies, from the workspace lock with --locked, and
checks that GPUI is not among them. Its `harness` example is the fixture
executable: the real update helper plus commands to sign fixtures and report a
restart. Use --offline to prohibit Cargo downloads; otherwise only normal Cargo
dependency downloads are needed. The repository toolchain must already be
installed (CI runs rustup show first).

Linux additionally exercises the real helper with offline, deterministically
signed fixtures in a private HOME. Run as a non-root user for those tests.
No daemon, desktop, network update endpoint, or production signing key is used.
Binaries and fixture homes are temporary; no repository file is modified and
nothing is published. Rust source paths are remapped in binaries.
"""

import argparse
import gzip
import hashlib
import io
import json
import os
from pathlib import Path
import selectors
import subprocess
import sys
import tarfile
import tempfile
import time
import tomllib


# Public half of the deliberately public [42; 32] unit-test signing seed.
PUBLIC_KEY = "197f6b23e16c8532c6abc838facd5ea789be0c76b2920334039bfa8b3d368d61"
VERSION = "20260920.1"
PACKAGE = "sheprd-updater"


def linux_helper_tests(binary, parent):
    target = {"x86_64": "x86_64", "aarch64": "aarch64"}[os.uname().machine]
    target += "-unknown-linux-gnu"
    old = binary.read_bytes()
    # ELF permits trailing bytes: the replacement remains our native fixture
    # executable, but byte comparison distinguishes it from the old installation.
    new = old + b"updater-test-replacement\n"
    for case in ("commit", "cancel", "wrong-token", "bad-signature", "bad-archive",
                 "stale-version", "unsafe-stage", "symlink-stage", "spawn-failure"):
        with tempfile.TemporaryDirectory(prefix="helper-", dir=parent) as directory:
            home = Path(directory).resolve()
            executable = home / "herdr-gpui"
            executable.write_bytes(old)
            executable.chmod(0o700)
            stage = Path(tempfile.mkdtemp(prefix=".herdr-update-", dir=home))
            marker = home / "restarted.json"
            environment = {"HOME": str(home), "PATH": "/usr/bin:/bin", "LC_ALL": "C"}
            payload_name = f"sheprd-20260920.2-{target}"
            buffer = io.BytesIO()
            with tarfile.open(fileobj=buffer, mode="w", format=tarfile.USTAR_FORMAT) as archive:
                entry = tarfile.TarInfo(payload_name)
                entry.mode = 0o700
                entry.size = len(new)
                archive.addfile(entry, io.BytesIO(new))
            archive_bytes = gzip.compress(buffer.getvalue(), mtime=0)
            (stage / "archive.tar.gz").write_bytes(archive_bytes)
            manifest = json.dumps({"schema": 1, "version": "20260920.2", "assets": [{
                "target": target, "name": payload_name + "-update.tar.gz",
                "size": len(archive_bytes), "sha256": hashlib.sha256(archive_bytes).hexdigest(),
            }]}).encode()
            signature = json.loads(subprocess.run(
                [executable, "--harness-sign"], input=manifest, capture_output=True,
                env=environment, cwd=home, check=True, timeout=10,
            ).stdout)
            argument = b"space and non-UTF8: \xff"
            request = {
                "current_version": VERSION, "version": "20260920.2",
                "manifest": list(manifest), "signature": signature,
                "args": [list(b"--harness-restarted"), list(os.fsencode(marker)), list(argument)],
                "cwd": list(os.fsencode(home)), "token": [42] * 32,
            }
            if case == "bad-signature":
                request["signature"][0] ^= 1
            elif case == "bad-archive":
                (stage / "archive.tar.gz").write_bytes(b"not the signed archive")
            elif case == "stale-version":
                request["current_version"] = "20260919.1"
            elif case == "unsafe-stage":
                stage.chmod(0o755)
            elif case == "spawn-failure":
                request["cwd"] = list(os.fsencode(home / "missing"))
            (stage / "request.json").write_text(json.dumps(request))
            (stage / "request.json").chmod(0o600)
            stage_arg = stage
            if case == "symlink-stage":
                stage_arg = home / ".herdr-update-link"
                stage_arg.symlink_to(stage, target_is_directory=True)
            rejected = case in {"bad-signature", "bad-archive", "stale-version",
                                "unsafe-stage", "symlink-stage"}
            with subprocess.Popen(
                [executable, "--herdr-apply-update", stage_arg], env=environment, cwd=home,
                stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            ) as helper:
                try:
                    if not rejected:
                        with selectors.DefaultSelector() as selector:
                            selector.register(helper.stdout, selectors.EVENT_READ)
                            assert selector.select(15), f"{case}: helper READY timed out"
                        assert os.read(helper.stdout.fileno(), 6) == b"READY\n", case
                        assert executable.read_bytes() == old, case
                        assert not marker.exists(), case
                    instruction = b"COMMIT\n" + bytes([43 if case == "wrong-token" else 42]) * 32
                    if case == "cancel" or rejected:
                        instruction = b""
                    stdout, stderr = helper.communicate(instruction, timeout=15)
                except BaseException:
                    helper.kill()
                    helper.communicate()
                    raise
            success = case == "commit"
            assert (helper.returncode == 0) == success, (case, helper.returncode, stderr)
            assert not stdout, (case, stdout)
            assert executable.read_bytes() == (new if success else old), case
            backup = stage / "previous-installation"
            if success:
                assert backup.read_bytes() == old
                deadline = time.monotonic() + 5
                # Atomicity is not assumed for the fixture's tiny report write.
                while True:
                    try:
                        report = json.loads(marker.read_bytes())
                        break
                    except (FileNotFoundError, json.JSONDecodeError):
                        if time.monotonic() >= deadline:
                            raise
                        time.sleep(0.01)
                assert report == {"argument": list(argument), "cwd": list(os.fsencode(home))}
                assert "Update installed" in (stage / "install-result.txt").read_text()
            else:
                assert not marker.exists(), case
                assert not backup.exists(), case
                if case == "spawn-failure":
                    assert "restart failed" in (stage / "install-result.txt").read_text()
            print(f"Linux helper: {case} passed", flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--offline", action="store_true", help="Use only cached Cargo dependencies")
    args = parser.parse_args()
    if not __debug__:
        parser.error("Python optimization disables test assertions; run without -O/PYTHONOPTIMIZE")
    if sys.platform == "linux" and os.geteuid() == 0:
        parser.error("Linux helper tests require a non-root user; do not run with sudo")
    root = Path(__file__).resolve().parents[1]
    channel = tomllib.loads((root / "rust-toolchain.toml").read_text())["toolchain"]["channel"]
    with tempfile.TemporaryDirectory(prefix="sheprd-updater-tests-") as directory:
        temporary = Path(directory).resolve()
        environment = dict(os.environ)
        environment.pop("RUSTFLAGS", None)
        environment.update({
            "HERDR_RELEASE_VERSION": VERSION, "HERDR_UPDATE_PUBLIC_KEY": PUBLIC_KEY,
            "CARGO_TARGET_DIR": str(temporary / "target"),
            "CARGO_ENCODED_RUSTFLAGS": f"--remap-path-prefix={root}=/herdr-source",
        })
        cargo = ["rustup", "run", channel, "cargo"]
        offline = ["--offline"] if args.offline else []

        def run(arguments, **kwargs):
            return subprocess.run(cargo + arguments + ["--locked"] + offline, cwd=root,
                                  env=environment, check=True, **kwargs)

        print(f"Updater-only harness: Rust {channel}, {sys.platform}", flush=True)
        tree = run(["tree", "-p", PACKAGE, "--edges", "normal,build", "--prefix", "none",
                    "--target", "all"], capture_output=True, text=True).stdout
        names = {line.split()[0] for line in tree.splitlines() if line.strip()}
        assert not any(name.startswith("gpui") for name in names), "sheprd-updater depends on GPUI"
        run(["build", "-p", PACKAGE, "--example", "harness"])
        binary = temporary / "target/debug/examples/harness"
        identity = json.loads(subprocess.run([binary, "--harness-identity"], capture_output=True,
                                             timeout=10, check=True).stdout)
        assert identity == {"version": VERSION, "key": PUBLIC_KEY, "fixture_key": PUBLIC_KEY}, identity
        invalid = subprocess.run([binary, "--herdr-apply-update"], timeout=10, check=False)
        assert invalid.returncode == 1, "Malformed helper invocation must fail closed"
        if sys.platform == "linux":
            linux_helper_tests(binary, temporary)
        else:
            print("Linux helper handshake is Linux-only; running shared installer unit tests.", flush=True)
        run(["test", "-p", PACKAGE, "--all-targets"])
    return 0


if __name__ == "__main__":
    sys.exit(main())
