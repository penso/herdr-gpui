import json
import os
from pathlib import Path
import shlex
import subprocess
import tempfile
import unittest


SCRIPTS = Path(__file__).resolve().parents[1]


class LinuxSmokeTests(unittest.TestCase):
    def test_container_checks_are_passed_intact_for_each_architecture(self):
        # Execute the outer shell as well as parsing the inner shell: bash -n
        # alone misses quotes that turn part of the payload into a command.
        for architecture in ("x86_64", "aarch64"):
            with self.subTest(architecture=architecture), tempfile.TemporaryDirectory() as directory:
                work = Path(directory)
                log = work / "docker.jsonl"
                docker = work / "docker"
                docker.write_text(
                    "#!/usr/bin/env python3\n"
                    "import json, os, subprocess, sys\n"
                    "args = sys.argv[1:]\n"
                    "check = args[args.index('-e') + 1]\n"
                    "assert check.startswith('CHECK=')\n"
                    "subprocess.run(['bash', '-n'], input=check[6:], text=True, check=True)\n"
                    "subprocess.run(['bash', '-n'], input=args[-1], text=True, check=True)\n"
                    "with open(os.environ['DOCKER_LOG'], 'a') as output:\n"
                    "    output.write(json.dumps(args) + '\\n')\n"
                )
                docker.chmod(0o755)
                env = dict(os.environ, PATH=f"{work}{os.pathsep}{os.environ['PATH']}",
                           DOCKER_LOG=str(log))
                result = subprocess.run(
                    ["bash", str(SCRIPTS / "smoke-linux-packages.sh"),
                     "20261102.1", f"{architecture}-unknown-linux-gnu", str(work)],
                    env=env, capture_output=True, text=True, timeout=30,
                )
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                calls = [json.loads(line) for line in log.read_text().splitlines()]
                images = [args[args.index('bash') - 1].split(":")[0] for args in calls]
                expected = ["ubuntu", "debian", "fedora"]
                if architecture == "x86_64":
                    expected.append("archlinux")
                self.assertEqual(images, expected)
                fedora_commands = calls[images.index("fedora")][-1]
                setup = fedora_commands.split('bash -c "$CHECK"', 1)[0]
                self.assertIn("diffutils", shlex.split(setup, comments=True))
                checks = [args[args.index('-e') + 1][6:] for args in calls]
                for check in checks:
                    self.assertEqual(check, checks[0])
                    self.assertIn('/usr/bin/herdr-core --version', check)
                    self.assertIn('linked=$(ldd "/usr/bin/$program")', check)
                    self.assertIn('libvulkan.so.1 libwayland-client.so.0', check)


if __name__ == "__main__":
    unittest.main()
