import contextlib
import json
import os
import select
import shutil
import socket
import subprocess
import sys
import tempfile
import threading
import unittest
import uuid
from pathlib import Path

ROOT = Path(__file__).absolute().parent.parent


class SecretServiceTests(unittest.TestCase):
    @unittest.skipUnless(sys.platform == "linux", "Linux Secret Service test")
    def test_real_service_in_long_special_checkout_path(self) -> None:
        with tempfile.TemporaryDirectory(dir=ROOT) as temporary:
            project = Path(temporary) / ("long,%" + "x" * 110)
            (project / "scripts").mkdir(parents=True)
            wrapper = project / "scripts/with-secret-service"
            shutil.copy2(ROOT / "scripts/with-secret-service", wrapper)
            result = subprocess.run(
                [str(wrapper), "gdbus", "call", "--session", "--timeout", "10",
                 "--dest", "org.freedesktop.DBus", "--object-path", "/org/freedesktop/DBus",
                 "--method", "org.freedesktop.DBus.ListNames"],
                text=True, capture_output=True, timeout=20, check=False,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("org.freedesktop.secrets", result.stdout)

    def test_isolated_service_preserves_status_and_cleans_up(self) -> None:
        with tempfile.TemporaryDirectory(dir=ROOT) as temporary:
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
            long_root = directory / ("long," + "x" * 110)
            (long_root / "scripts").mkdir(parents=True)
            wrapper = long_root / "scripts/with-secret-service"
            shutil.copy2(ROOT / "scripts/with-secret-service", wrapper)
            for code in (0, 17):
                result = subprocess.run(
                    [
                        str(wrapper),
                        sys.executable,
                        "-c",
                        "import json, os, sys; print(json.dumps([os.environ['XDG_DATA_HOME'], "
                        "os.path.realpath(os.environ['XDG_RUNTIME_DIR']), os.environ['DBUS_SESSION_BUS_ADDRESS']])); "
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
                self.assertRegex(bus, r"^unix:path=/proc/[0-9]+/fd/3/b$")
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
        with tempfile.TemporaryDirectory(dir=ROOT) as temporary:
            ambient_fd = os.open(temporary, os.O_RDONLY | os.O_DIRECTORY)
            address = f"/proc/{os.getpid()}/fd/{ambient_fd}/b"
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
                    "cargo", "test", "--message-format=json", "--locked", "-p", "ejm-harness-auth",
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
                self.assertEqual(result.returncode, 0, result.stderr + result.stdout[-8192:])
                artifacts = [json.loads(line) for line in result.stdout.splitlines()
                             if line.startswith("{")]
                executables = [artifact["executable"] for artifact in artifacts
                               if artifact.get("reason") == "compiler-artifact"
                               and artifact.get("profile", {}).get("test")
                               and artifact.get("target", {}).get("name") == "ejm_harness_auth"
                               and artifact.get("executable")]
                self.assertEqual(len(executables), 1, result.stdout + result.stderr)
                with contextlib.ExitStack() as cleanup:
                    fake_dir = ROOT / f".s.{uuid.uuid4().hex[:8]}"
                    fake_dir.mkdir(mode=0o700)
                    cleanup.callback(shutil.rmtree, fake_dir)
                    (fake_dir / "r").mkdir(mode=0o700)
                    (fake_dir / "data").mkdir(mode=0o700)
                    fake_fd = os.open(fake_dir / "r", os.O_RDONLY | os.O_DIRECTORY)
                    fake_socket = socket.socket(socket.AF_UNIX)
                    fake_socket.bind(f"/proc/{os.getpid()}/fd/{fake_fd}/b")
                    fake_socket.listen()
                    fake_socket.settimeout(0.1)

                    def forward() -> None:
                        while not stopped.is_set():
                            try:
                                client, _ = fake_socket.accept()
                            except TimeoutError:
                                continue
                            with client, socket.socket(socket.AF_UNIX) as ambient:
                                connected = False
                                while not stopped.is_set():
                                    ready, _, _ = select.select(
                                        [client, ambient] if connected else [client], [], [], 0.1
                                    )
                                    if not ready:
                                        continue
                                    data = ready[0].recv(65536)
                                    if not data:
                                        break
                                    if not connected:
                                        ambient.connect(address)
                                        connected = True
                                    (ambient if ready[0] is client else client).sendall(data)

                    forwarder = threading.Thread(target=forward)
                    forwarder.start()
                    try:
                        forged = subprocess.run(
                            ["sh", "-c", 'exec 3< "$FAKE_RUNTIME"; '
                             'export XDG_RUNTIME_DIR="/proc/$$/fd/3"; '
                             'export DBUS_SESSION_BUS_ADDRESS="unix:path=$XDG_RUNTIME_DIR/b"; '
                             '"$TEST_EXECUTABLE" --exact '
                             'store::vault::tests::native_os_key_service_round_trip; '
                             'status=$?; exit "$status"'],
                            cwd=ROOT,
                            env={**env, "EJM_AUTH_SECRET_SERVICE_TEST_CHILD": "1",
                                 "XDG_DATA_HOME": str(fake_dir / "data"),
                                 "FAKE_RUNTIME": str(fake_dir / "r"),
                                 "TEST_EXECUTABLE": executables[0],
                                 "EJM_TEST_SECRET_BUS_PID": str(os.getpid()),
                                 "EJM_TEST_SECRET_SERVICE_PID": str(os.getpid())},
                            text=True,
                            capture_output=True,
                            timeout=20,
                            check=False,
                        )
                    finally:
                        stopped.set()
                        forwarder.join()
                        fake_socket.close()
                        os.close(fake_fd)
            finally:
                stopped.set()
                observer.join()
                listener.close()
                os.close(ambient_fd)
            self.assertFalse(contacted.is_set(), "credential test contacted the ambient bus")
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("1 passed", result.stdout)
            self.assertNotEqual(forged.returncode, 0, forged.stdout + forged.stderr)
            self.assertIn("unexpected isolated service process", forged.stdout + forged.stderr)


if __name__ == "__main__":
    unittest.main()
