import unittest

from tools.codex_probe import analyze, install_hooks, sanitize
from tools.codex_probe.events import HOOK_EVENT_NAMES


class HookEventNamesTests(unittest.TestCase):
    def test_all_probe_components_share_the_immutable_event_tuple(self):
        self.assertIsInstance(HOOK_EVENT_NAMES, tuple)
        self.assertEqual(
            HOOK_EVENT_NAMES,
            (
                "SessionStart",
                "SessionEnd",
                "UserPromptSubmit",
                "PreToolUse",
                "PermissionRequest",
                "PostToolUse",
                "PreCompact",
                "PostCompact",
                "SubagentStart",
                "SubagentStop",
                "Stop",
            ),
        )
        self.assertIs(sanitize.HOOK_EVENT_NAMES, HOOK_EVENT_NAMES)
        self.assertIs(analyze.HOOK_EVENT_NAMES, HOOK_EVENT_NAMES)
        self.assertIs(install_hooks.EVENT_NAMES, HOOK_EVENT_NAMES)


if __name__ == "__main__":
    unittest.main()
