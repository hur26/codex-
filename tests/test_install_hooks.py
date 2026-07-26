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

    def test_final_publish_does_not_overwrite_a_concurrent_recreation(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            config_path = root / "hooks.json"
            original = b'{"hooks": {}}\n'
            concurrent = b'{"changed": "in final publish window"}\n'
            rendered = '{"hooks": {"Stop": []}}\n'
            config_path.write_bytes(original)
            real_link = install_hooks.os.link
            injected = False

            def recreate_config_before_publish(
                source: str | bytes | Path,
                destination: str | bytes | Path,
                *args: object,
                **kwargs: object,
            ) -> None:
                nonlocal injected
                if Path(destination) == config_path:
                    config_path.write_bytes(concurrent)
                    injected = True
                real_link(source, destination, *args, **kwargs)

            with mock.patch.object(
                install_hooks.os,
                "link",
                side_effect=recreate_config_before_publish,
            ):
                with self.assertRaisesRegex(RuntimeError, "concurrent"):
                    install_hooks.apply_hooks(
                        config_path,
                        original,
                        rendered,
                    )

            self.assertTrue(injected)
            self.assertEqual(config_path.read_bytes(), concurrent)
            backups = list(root.glob("hooks.json.bak.*"))
            self.assertEqual(len(backups), 1)
            self.assertEqual(backups[0].read_bytes(), original)
            self.assertEqual(list(root.glob(".hooks.json.transaction.*")), [])
            self.assertEqual(list(root.glob(".hooks.json.*.tmp")), [])

    def test_transaction_detects_write_after_check_before_displacement(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            config_path = root / "hooks.json"
            original = b'{"hooks": {}}\n'
            concurrent = b'{"changed": "after final check"}\n'
            config_path.write_bytes(original)
            real_replace = install_hooks.os.replace
            injected = False

            def mutate_before_displacement(
                source: str | bytes | Path,
                destination: str | bytes | Path,
                *args: object,
                **kwargs: object,
            ) -> None:
                nonlocal injected
                destination_path = Path(destination)
                if (
                    Path(source) == config_path
                    and destination_path.name.startswith(
                        ".hooks.json.transaction."
                    )
                ):
                    config_path.write_bytes(concurrent)
                    injected = True
                real_replace(source, destination, *args, **kwargs)

            with mock.patch.object(
                install_hooks.os,
                "replace",
                side_effect=mutate_before_displacement,
            ):
                with self.assertRaisesRegex(RuntimeError, "changed"):
                    install_hooks.apply_hooks(
                        config_path,
                        original,
                        '{"hooks": {"Stop": []}}\n',
                    )

            self.assertTrue(injected)
            self.assertEqual(config_path.read_bytes(), concurrent)
            backups = list(root.glob("hooks.json.bak.*"))
            self.assertEqual(len(backups), 1)
            self.assertEqual(backups[0].read_bytes(), original)
            self.assertEqual(list(root.glob(".hooks.json.transaction.*")), [])
            self.assertEqual(list(root.glob(".hooks.json.*.tmp")), [])

    def test_publish_failure_restores_original_when_target_is_still_absent(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            config_path = root / "hooks.json"
            original = b'{"hooks": {}}\n'
            config_path.write_bytes(original)
            real_link = install_hooks.os.link
            failed_once = False

            def fail_first_publish(
                source: str | bytes | Path,
                destination: str | bytes | Path,
                *args: object,
                **kwargs: object,
            ) -> None:
                nonlocal failed_once
                if Path(destination) == config_path and not failed_once:
                    failed_once = True
                    raise OSError("simulated publish failure")
                real_link(source, destination, *args, **kwargs)

            with mock.patch.object(
                install_hooks.os,
                "link",
                side_effect=fail_first_publish,
            ):
                with self.assertRaisesRegex(OSError, "simulated publish failure"):
                    install_hooks.apply_hooks(
                        config_path,
                        original,
                        '{"hooks": {"Stop": []}}\n',
                    )

            self.assertEqual(config_path.read_bytes(), original)
            self.assertEqual(list(root.glob(".hooks.json.transaction.*")), [])
            self.assertEqual(list(root.glob(".hooks.json.*.tmp")), [])

    def test_hard_link_capability_is_verified_before_config_is_displaced(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            config_path = root / "hooks.json"
            original = b'{"hooks": {}}\n'
            config_path.write_bytes(original)
            real_link = install_hooks.os.link

            def reject_capability_probe(
                source: str | bytes | Path,
                destination: str | bytes | Path,
                *args: object,
                **kwargs: object,
            ) -> None:
                if ".hardlink-probe." in Path(destination).name:
                    raise OSError("hard links unavailable")
                real_link(source, destination, *args, **kwargs)

            with mock.patch.object(
                install_hooks.os,
                "link",
                side_effect=reject_capability_probe,
            ):
                with self.assertRaisesRegex(OSError, "hard links unavailable"):
                    install_hooks.apply_hooks(
                        config_path,
                        original,
                        '{"hooks": {"Stop": []}}\n',
                    )

            self.assertEqual(config_path.read_bytes(), original)
            self.assertEqual(list(root.glob(".hooks.json.transaction.*")), [])
            self.assertEqual(list(root.glob(".hooks.json.*.tmp")), [])

    def test_publish_and_restore_failure_reports_recovery_file(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            config_path = root / "hooks.json"
            original = b'{"hooks": {}}\n'
            config_path.write_bytes(original)
            real_link = install_hooks.os.link

            def fail_config_links(
                source: str | bytes | Path,
                destination: str | bytes | Path,
                *args: object,
                **kwargs: object,
            ) -> None:
                if Path(destination) == config_path:
                    raise OSError("simulated config link failure")
                real_link(source, destination, *args, **kwargs)

            with mock.patch.object(
                install_hooks.os,
                "link",
                side_effect=fail_config_links,
            ):
                with self.assertRaisesRegex(
                    install_hooks.TransactionRecoveryError,
                    r"\.hooks\.json\.transaction\.",
                ) as raised:
                    install_hooks.apply_hooks(
                        config_path,
                        original,
                        '{"hooks": {"Stop": []}}\n',
                    )

            displaced = list(root.glob(".hooks.json.transaction.*"))
            self.assertEqual(len(displaced), 1)
            self.assertIn(str(displaced[0]), str(raised.exception))
            self.assertEqual(displaced[0].read_bytes(), original)
            self.assertFalse(config_path.exists())
            self.assertEqual(list(root.glob(".hooks.json.*.tmp")), [])

    def test_apply_recovers_interrupted_transaction_before_installing(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            config_path = root / "hooks.json"
            displaced = root / ".hooks.json.transaction.interrupted"
            original = b'{"custom": "preserved", "hooks": {}}\n'
            displaced.write_bytes(original)

            result = self._run_installer(
                config_path,
                "python probe.py",
                apply=True,
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertFalse(displaced.exists())
            merged = json.loads(config_path.read_text(encoding="utf-8"))
            self.assertEqual(merged["custom"], "preserved")
            backups = list(root.glob("hooks.json.bak.*"))
            self.assertEqual(len(backups), 1)
            self.assertEqual(backups[0].read_bytes(), original)

    def test_dry_run_reports_interrupted_transaction_without_writing(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            config_path = root / "hooks.json"
            displaced = root / ".hooks.json.transaction.interrupted"
            original = b'{"hooks": {}}\n'
            displaced.write_bytes(original)

            result = self._run_installer(config_path, "python probe.py")

            self.assertNotEqual(result.returncode, 0)
            self.assertIn(str(displaced), result.stderr)
            self.assertIn("--apply", result.stderr)
            self.assertFalse(config_path.exists())
            self.assertEqual(displaced.read_bytes(), original)
            self.assertEqual(list(root.glob("hooks.json.bak.*")), [])

    def test_apply_refuses_to_guess_between_multiple_recovery_files(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            config_path = root / "hooks.json"
            first = root / ".hooks.json.transaction.first"
            second = root / ".hooks.json.transaction.second"
            first_content = b'{"source": "first"}\n'
            second_content = b'{"source": "second"}\n'
            first.write_bytes(first_content)
            second.write_bytes(second_content)

            result = self._run_installer(
                config_path,
                "python probe.py",
                apply=True,
            )

            self.assertNotEqual(result.returncode, 0)
            self.assertIn(str(first), result.stderr)
            self.assertIn(str(second), result.stderr)
            self.assertFalse(config_path.exists())
            self.assertEqual(first.read_bytes(), first_content)
            self.assertEqual(second.read_bytes(), second_content)
            self.assertEqual(list(root.glob("hooks.json.bak.*")), [])

    def test_existing_config_with_one_transaction_is_never_ignored(self):
        for apply in (False, True):
            with self.subTest(apply=apply):
                with tempfile.TemporaryDirectory() as temp_dir:
                    root = Path(temp_dir)
                    config_path = root / "hooks.json"
                    displaced = root / ".hooks.json.transaction.pending"
                    config_content = b'{"source": "current"}\n'
                    displaced_content = b'{"source": "pending"}\n'
                    config_path.write_bytes(config_content)
                    displaced.write_bytes(displaced_content)

                    result = self._run_installer(
                        config_path,
                        "python probe.py",
                        apply=apply,
                    )

                    self.assertNotEqual(result.returncode, 0)
                    self.assertIn(str(config_path), result.stderr)
                    self.assertIn(str(displaced), result.stderr)
                    self.assertEqual(config_path.read_bytes(), config_content)
                    self.assertEqual(displaced.read_bytes(), displaced_content)
                    self.assertEqual(list(root.glob("hooks.json.bak.*")), [])

    def test_existing_config_with_multiple_transactions_is_never_ignored(self):
        for apply in (False, True):
            with self.subTest(apply=apply):
                with tempfile.TemporaryDirectory() as temp_dir:
                    root = Path(temp_dir)
                    config_path = root / "hooks.json"
                    first = root / ".hooks.json.transaction.first"
                    second = root / ".hooks.json.transaction.second"
                    config_content = b'{"source": "current"}\n'
                    first_content = b'{"source": "first"}\n'
                    second_content = b'{"source": "second"}\n'
                    config_path.write_bytes(config_content)
                    first.write_bytes(first_content)
                    second.write_bytes(second_content)

                    result = self._run_installer(
                        config_path,
                        "python probe.py",
                        apply=apply,
                    )

                    self.assertNotEqual(result.returncode, 0)
                    self.assertIn(str(config_path), result.stderr)
                    self.assertIn(str(first), result.stderr)
                    self.assertIn(str(second), result.stderr)
                    self.assertEqual(config_path.read_bytes(), config_content)
                    self.assertEqual(first.read_bytes(), first_content)
                    self.assertEqual(second.read_bytes(), second_content)
                    self.assertEqual(list(root.glob("hooks.json.bak.*")), [])

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

    def test_recovery_path_survives_publish_restore_and_cleanup_failures(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            config_path = root / "hooks.json"
            original = b'{"hooks": {}}\n'
            config_path.write_bytes(original)
            real_link = install_hooks.os.link
            real_unlink = Path.unlink

            def fail_config_links(
                source: str | bytes | Path,
                destination: str | bytes | Path,
                *args: object,
                **kwargs: object,
            ) -> None:
                if Path(destination) == config_path:
                    raise OSError("simulated config link failure")
                real_link(source, destination, *args, **kwargs)

            def fail_config_temp_cleanup(
                path: Path,
                *args: object,
                **kwargs: object,
            ) -> None:
                if (
                    path.name.startswith(".hooks.json.")
                    and path.name.endswith(".tmp")
                    and ".backup." not in path.name
                ):
                    raise OSError("simulated temporary cleanup failure")
                real_unlink(path, *args, **kwargs)

            with (
                mock.patch.object(
                    install_hooks.os,
                    "link",
                    side_effect=fail_config_links,
                ),
                mock.patch.object(
                    Path,
                    "unlink",
                    autospec=True,
                    side_effect=fail_config_temp_cleanup,
                ),
            ):
                with self.assertRaises(
                    install_hooks.TransactionRecoveryError
                ) as raised:
                    install_hooks.apply_hooks(
                        config_path,
                        original,
                        '{"hooks": {"Stop": []}}\n',
                    )

            displaced = list(root.glob(".hooks.json.transaction.*"))
            self.assertEqual(len(displaced), 1)
            self.assertIn(str(displaced[0]), str(raised.exception))
            self.assertEqual(displaced[0].read_bytes(), original)
            self.assertFalse(config_path.exists())


if __name__ == "__main__":
    unittest.main()
