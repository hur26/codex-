import io
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from tools.codex_probe import probe


class ProbeCliTests(unittest.TestCase):
    def test_writes_one_sanitized_event_file(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            env = os.environ.copy()
            env["CODEX_HALO_PROBE_DIR"] = temp_dir
            payload = {
                "hook_type": "PreToolUse",
                "thread_id": "stable-thread-id",
                "arguments": {"cmd": "SECRET COMMAND"},
            }

            result = subprocess.run(
                [sys.executable, "-m", "tools.codex_probe.probe"],
                input=json.dumps(payload),
                text=True,
                capture_output=True,
                env=env,
                check=False,
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            files = list(Path(temp_dir).glob("*.json"))
            self.assertEqual(len(files), 1)
            saved = files[0].read_text(encoding="utf-8")
            self.assertNotIn("stable-thread-id", saved)
            self.assertNotIn("SECRET COMMAND", saved)
            self.assertEqual(json.loads(saved)["hook_type"], "PreToolUse")

    def test_direct_script_invocation_works_outside_repository(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            env = os.environ.copy()
            env["CODEX_HALO_PROBE_DIR"] = temp_dir
            script = (
                Path(__file__).resolve().parents[1]
                / "tools"
                / "codex_probe"
                / "probe.py"
            )
            result = subprocess.run(
                [sys.executable, str(script)],
                input=json.dumps({"hook_type": "Stop", "thread_id": "thread-a"}),
                text=True,
                capture_output=True,
                cwd=temp_dir,
                env=env,
                check=False,
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(len(list(Path(temp_dir).glob("*.json"))), 1)

    def test_invalid_json_exits_successfully_without_writing(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            env = os.environ.copy()
            env["CODEX_HALO_PROBE_DIR"] = temp_dir
            result = subprocess.run(
                [sys.executable, "-m", "tools.codex_probe.probe"],
                input="{not-json",
                text=True,
                capture_output=True,
                env=env,
                check=False,
            )

            self.assertEqual(result.returncode, 0)
            self.assertEqual(list(Path(temp_dir).glob("*.json")), [])

    def test_oversized_valid_json_exits_successfully_without_writing(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            env = os.environ.copy()
            env["CODEX_HALO_PROBE_DIR"] = temp_dir
            payload = {
                "hook_type": "Stop",
                "arguments": {"content": "x" * probe.MAX_INPUT_BYTES},
            }

            result = subprocess.run(
                [sys.executable, "-m", "tools.codex_probe.probe"],
                input=json.dumps(payload),
                text=True,
                capture_output=True,
                env=env,
                check=False,
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(list(Path(temp_dir).glob("*.json")), [])

    def test_publishes_valid_event_with_atomic_replace(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            payload = json.dumps(
                {"hook_type": "Stop", "thread_id": "thread-a"}
            ).encode("utf-8")
            stdin = io.TextIOWrapper(io.BytesIO(payload), encoding="utf-8")
            real_replace = os.replace

            with (
                mock.patch.dict(
                    os.environ,
                    {"CODEX_HALO_PROBE_DIR": temp_dir},
                ),
                mock.patch.object(probe.sys, "stdin", stdin),
                mock.patch.object(
                    probe.os,
                    "replace",
                    wraps=real_replace,
                ) as replace_spy,
            ):
                self.assertEqual(probe.main(), 0)

            replace_spy.assert_called_once()
            files = list(Path(temp_dir).glob("*.json"))
            self.assertEqual(len(files), 1)
            self.assertEqual(
                json.loads(files[0].read_text(encoding="utf-8"))["hook_type"],
                "Stop",
            )
            self.assertEqual(list(Path(temp_dir).glob("*.tmp")), [])

    def test_environment_override_does_not_resolve_home_directory(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            payload = json.dumps({"hook_type": "Stop"}).encode("utf-8")
            stdin = io.TextIOWrapper(io.BytesIO(payload), encoding="utf-8")

            with (
                mock.patch.dict(
                    os.environ,
                    {"CODEX_HALO_PROBE_DIR": temp_dir},
                ),
                mock.patch.object(probe.sys, "stdin", stdin),
                mock.patch.object(
                    probe.Path,
                    "home",
                    side_effect=RuntimeError("home unavailable"),
                ),
            ):
                self.assertEqual(probe.main(), 0)

            self.assertEqual(len(list(Path(temp_dir).glob("*.json"))), 1)


if __name__ == "__main__":
    unittest.main()
