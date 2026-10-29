import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).absolute().parent.parent


class SecretServiceTests(unittest.TestCase):
    def test_isolated_service_preserves_status_and_cleans_up(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            for name, source in {
                "dbus-run-session": (
                    '#!/bin/sh\nshift\nexport DBUS_SESSION_BUS_ADDRESS=test-bus\nexec "$@"\n'
                ),
                "gnome-keyring-daemon": (
                    f"#!{sys.executable}\n"
                    "import os, pathlib, signal, sys, time\n"
                    "signal.signal(signal.SIGTERM, lambda *_: sys.exit(0))\n"
                    "pathlib.Path(os.environ['XDG_RUNTIME_DIR'], 'ready').touch()\n"
                    "while True: time.sleep(1)\n"
                ),
                "gdbus": (
                    f"#!{sys.executable}\n"
                    "import os, pathlib, sys, time\n"
                    "if os.environ.get('FAIL_READY'): sys.exit(23)\n"
                    "p = pathlib.Path(os.environ['XDG_RUNTIME_DIR'], 'ready')\n"
                    "for _ in range(100):\n"
                    "    if p.exists(): sys.exit(0)\n"
                    "    time.sleep(.01)\n"
                    "sys.exit(24)\n"
                ),
            }.items():
                path = directory / name
                path.write_text(source, encoding="utf-8")
                path.chmod(0o755)
            env = {**os.environ, "PATH": str(directory) + os.pathsep + os.environ["PATH"]}
            for code in (0, 17):
                result = subprocess.run(
                    [
                        str(ROOT / "scripts/with-secret-service"),
                        sys.executable,
                        "-c",
                        "import json, os, sys; print(json.dumps([os.environ['XDG_DATA_HOME'], "
                        "os.environ['XDG_RUNTIME_DIR'], os.environ['DBUS_SESSION_BUS_ADDRESS']])); "
                        f"sys.exit({code})",
                    ],
                    cwd=ROOT,
                    env=env,
                    text=True,
                    capture_output=True,
                    timeout=15,
                    check=False,
                )
                self.assertEqual(result.returncode, code, result.stderr)
                data, runtime, bus = json.loads(result.stdout)
                self.assertEqual(bus, "test-bus")
                self.assertEqual(Path(data).parent, Path(runtime).parent)
                self.assertFalse(Path(data).parent.exists())
            failed_result = subprocess.run(
                [str(ROOT / "scripts/with-secret-service"), sys.executable, "-c", "exit(0)"],
                cwd=ROOT,
                env={**env, "FAIL_READY": "1"},
                timeout=15,
                check=False,
            )
            self.assertEqual(failed_result.returncode, 23)


if __name__ == "__main__":
    unittest.main()
