import json
import os
import socket
import subprocess
import sys
import tempfile
import threading
import unittest
from pathlib import Path

ROOT = Path(__file__).absolute().parent.parent


class SecretServiceTests(unittest.TestCase):
    def test_isolated_service_preserves_status_and_cleans_up(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            for name, source in {
                "dbus-daemon": (
                    f"#!{sys.executable}\n"
                    "import os, pathlib, signal, socket, sys, time\n"
                    "signal.signal(signal.SIGTERM, lambda *_: sys.exit(0))\n"
                    "server = socket.socket(socket.AF_UNIX)\n"
                    "server.bind(str(pathlib.Path(os.environ['XDG_RUNTIME_DIR'], 'b')))\n"
                    "server.listen()\n"
                    "while True: time.sleep(1)\n"
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
                self.assertEqual(bus, f"unix:path={runtime}/b")
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

    @unittest.skipUnless(sys.platform == "linux", "Linux Secret Service test")
    def test_direct_cargo_entry_uses_no_ambient_bus(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            address = str(Path(temporary) / "b")
            listener = socket.socket(socket.AF_UNIX)
            listener.bind(address)
            listener.listen()
            listener.settimeout(0.1)
            stopped = threading.Event()
            contacted = threading.Event()

            def observe_ambient() -> None:
                while not stopped.is_set():
                    try:
                        connection, _ = listener.accept()
                    except TimeoutError:
                        continue
                    with connection:
                        contacted.set()

            observer = threading.Thread(target=observe_ambient)
            observer.start()
            try:
                env = {**os.environ, "DBUS_SESSION_BUS_ADDRESS": f"unix:path={address}"}
                env.pop("EJM_AUTH_SECRET_SERVICE_TEST_CHILD", None)
                command = [
                    "cargo", "test", "--locked", "-p", "ejm-harness-auth",
                    "--lib", "native_os_key_service_round_trip",
                ]
                result = subprocess.run(
                    command,
                    cwd=ROOT,
                    env=env,
                    text=True,
                    capture_output=True,
                    timeout=600,
                    check=False,
                )
                forged = subprocess.run(
                    command,
                    cwd=ROOT,
                    env={**env, "EJM_AUTH_SECRET_SERVICE_TEST_CHILD": "1", "XDG_RUNTIME_DIR": temporary},
                    text=True,
                    capture_output=True,
                    timeout=600,
                    check=False,
                )
            finally:
                stopped.set()
                observer.join()
                listener.close()
            self.assertFalse(contacted.is_set(), "credential test contacted the ambient bus")
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("1 passed", result.stdout)
            self.assertNotEqual(forged.returncode, 0, forged.stdout + forged.stderr)


if __name__ == "__main__":
    unittest.main()
