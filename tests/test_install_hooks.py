import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from tools.codex_probe.install_hooks import EVENT_NAMES, merge_hooks


class MergeHooksTests(unittest.TestCase):
    def test_preserves_existing_hooks_and_is_idempotent(self):
        existing = {
            "version": 1,
            "hooks": {
                "PreToolUse": [
                    {
                        "matcher": "",
                        "hooks": [
                            {"type": "command", "command": "python old-hook.py"}
                        ],
                    }
                ]
            },
        }
        command = '"C:\\Python\\python.exe" -m tools.codex_probe.probe'

        first = merge_hooks(existing, command)
        second = merge_hooks(first, command)

        self.assertEqual(first, second)
        self.assertEqual(existing["version"], first["version"])
        self.assertEqual(
            first["hooks"]["PreToolUse"][0]["hooks"][0]["command"],
            "python old-hook.py",
        )
        for event_name in EVENT_NAMES:
            commands = [
                hook["command"]
                for entry in first["hooks"][event_name]
                for hook in entry.get("hooks", [])
            ]
            self.assertEqual(commands.count(command), 1)


class InstallerCliTests(unittest.TestCase):
    def _run_installer(
        self,
        config_path: Path,
        command: str,
        *,
        apply: bool = False,
    ) -> subprocess.CompletedProcess[str]:
        arguments = [
            sys.executable,
            "-m",
            "tools.codex_probe.install_hooks",
            "--config",
            str(config_path),
            "--command",
            command,
        ]
        if apply:
            arguments.append("--apply")
        return subprocess.run(
            arguments,
            text=True,
            capture_output=True,
            check=False,
        )

    def test_defaults_to_dry_run_without_modifying_or_backing_up_config(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            config_path = Path(temp_dir) / "hooks.json"
            original = '{"custom": "preserved", "hooks": {}}\n'
            config_path.write_text(original, encoding="utf-8")

            result = self._run_installer(config_path, "python probe.py")

            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(config_path.read_text(encoding="utf-8"), original)
            self.assertEqual(list(config_path.parent.glob("hooks.json.bak.*")), [])
            preview = json.loads(result.stdout)
            for event_name in EVENT_NAMES:
                commands = [
                    hook["command"]
                    for entry in preview["hooks"][event_name]
                    for hook in entry.get("hooks", [])
                ]
                self.assertEqual(commands.count("python probe.py"), 1)

    def test_apply_creates_exact_timestamped_backup_before_writing(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            config_path = Path(temp_dir) / "hooks.json"
            original = (
                '{\n'
                '  "custom": "preserved",\n'
                '  "hooks": {"Stop": [{"hooks": ['
                '{"type": "command", "command": "python old.py"}]}]}\n'
                '}\n'
            )
            config_path.write_text(original, encoding="utf-8")

            result = self._run_installer(
                config_path,
                "python probe.py",
                apply=True,
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            backups = list(config_path.parent.glob("hooks.json.bak.*"))
            self.assertEqual(len(backups), 1)
            self.assertEqual(backups[0].read_text(encoding="utf-8"), original)

            merged = json.loads(config_path.read_text(encoding="utf-8"))
            self.assertEqual(merged["custom"], "preserved")
            self.assertEqual(
                merged["hooks"]["Stop"][0]["hooks"][0]["command"],
                "python old.py",
            )
            for event_name in EVENT_NAMES:
                commands = [
                    hook["command"]
                    for entry in merged["hooks"][event_name]
                    for hook in entry.get("hooks", [])
                ]
                self.assertEqual(commands.count("python probe.py"), 1)


if __name__ == "__main__":
    unittest.main()
