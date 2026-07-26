import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from tools.codex_probe import install_hooks
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

    def test_apply_aborts_if_config_changes_while_backup_is_created(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            config_path = Path(temp_dir) / "hooks.json"
            original = b'{"hooks": {}}\n'
            concurrent = b'{"changed": "by another process"}\n'
            config_path.write_bytes(original)
            create_backup = install_hooks._create_backup

            def create_backup_then_mutate(path: Path, content: bytes) -> Path:
                backup = create_backup(path, content)
                path.write_bytes(concurrent)
                return backup

            with mock.patch.object(
                install_hooks,
                "_create_backup",
                side_effect=create_backup_then_mutate,
            ):
                with self.assertRaisesRegex(RuntimeError, "changed"):
                    install_hooks.apply_hooks(
                        config_path,
                        original,
                        '{"hooks": {"Stop": []}}\n',
                    )

            self.assertEqual(config_path.read_bytes(), concurrent)
            backups = list(config_path.parent.glob("hooks.json.bak.*"))
            self.assertEqual(len(backups), 1)
            self.assertEqual(backups[0].read_bytes(), original)

    def test_rejects_symlink_without_changing_target_or_replacing_link(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            target = root / "real-hooks.json"
            link = root / "hooks.json"
            original = b'{"hooks": {}}\n'
            target.write_bytes(original)
            try:
                link.symlink_to(target)
            except OSError as error:
                self.skipTest(f"Symbolic links are unavailable: {error}")

            result = self._run_installer(link, "python probe.py", apply=True)

            self.assertNotEqual(result.returncode, 0)
            self.assertTrue(link.is_symlink())
            self.assertEqual(target.read_bytes(), original)
            self.assertEqual(list(root.glob("hooks.json.bak.*")), [])


class AtomicBackupTests(unittest.TestCase):
    def test_replace_failure_leaves_no_partial_backup_or_temporary_file(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            config_path = root / "hooks.json"
            original = b'{"hooks": {}}\n'
            config_path.write_bytes(original)

            with mock.patch.object(
                install_hooks.os,
                "replace",
                side_effect=OSError("simulated replacement failure"),
            ):
                with self.assertRaisesRegex(OSError, "simulated replacement failure"):
                    install_hooks._create_backup(config_path, original)

            self.assertEqual(list(root.glob("hooks.json.bak.*")), [])
            self.assertEqual(
                [path for path in root.iterdir() if path != config_path],
                [],
            )


if __name__ == "__main__":
    unittest.main()
