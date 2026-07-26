import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


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


if __name__ == "__main__":
    unittest.main()
