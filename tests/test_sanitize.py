import hashlib
import json
import unittest
from datetime import datetime, timezone

from tools.codex_probe.sanitize import summarize_event


class SummarizeEventTests(unittest.TestCase):
    def test_redacts_values_and_fingerprints_candidate_ids(self):
        payload = {
            "hook_type": "PreToolUse",
            "tool_name": "exec_command",
            "thread_id": "thread-secret-123",
            "cwd": r"D:\Private\Client",
            "arguments": {
                "cmd": "print the production token",
                "content": "SECRET-CODE-CONTENT",
            },
            "env": {"SECRET_KEY_NAME": "SECRET_VALUE"},
            "message": "private prompt",
        }

        summary = summarize_event(
            payload,
            received_at=datetime(2026, 7, 26, 8, 0, tzinfo=timezone.utc),
        )
        serialized = json.dumps(summary, ensure_ascii=False)

        self.assertEqual(summary["hook_type"], "PreToolUse")
        self.assertEqual(summary["tool_name"], "exec_command")
        self.assertEqual(summary["top_level_keys"], sorted(payload))
        self.assertEqual(summary["received_at"], "2026-07-26T08:00:00+00:00")
        self.assertNotIn("thread-secret-123", serialized)
        self.assertNotIn(r"D:\Private\Client", serialized)
        self.assertNotIn("SECRET-CODE-CONTENT", serialized)
        self.assertNotIn("private prompt", serialized)
        self.assertNotIn("production token", serialized)
        self.assertNotIn("SECRET_KEY_NAME", serialized)
        self.assertNotIn("SECRET_VALUE", serialized)

        identities = {item["path"]: item for item in summary["identity_candidates"]}
        self.assertIn("$.thread_id", identities)
        self.assertIn("$.cwd", identities)
        self.assertRegex(identities["$.thread_id"]["fingerprint"], r"^[0-9a-f]{16}$")
        self.assertEqual(identities["$.thread_id"]["length"], 17)

    def test_records_shape_without_scalar_values(self):
        summary = summarize_event(
            {"hook_type": "Stop", "nested": {"items": [1, "secret"]}},
            received_at=datetime(2026, 7, 26, 8, 1, tzinfo=timezone.utc),
        )

        paths = {item["path"]: item["kind"] for item in summary["shape"]}
        self.assertEqual(paths["$"], "object")
        self.assertEqual(paths["$.nested"], "object")
        self.assertEqual(paths["$.nested.items"], "array")
        self.assertEqual(paths["$.nested.items[0]"], "number")
        self.assertEqual(paths["$.nested.items[1]"], "string")

    def test_bounds_deeply_nested_payloads(self):
        nested = "secret"
        for _ in range(2_000):
            nested = {"nested": nested}

        summary = summarize_event(
            {"hook_type": "Stop", "nested": nested},
            received_at=datetime(2026, 7, 26, 8, 2, tzinfo=timezone.utc),
        )

        self.assertLessEqual(len(summary["shape"]), 512)
        self.assertTrue(any(item.get("truncated") for item in summary["shape"]))

    def test_limits_large_containers(self):
        summary = summarize_event(
            {"hook_type": "Stop", "items": list(range(10_000))},
            received_at=datetime(2026, 7, 26, 8, 3, tzinfo=timezone.utc),
        )

        self.assertLessEqual(len(summary["shape"]), 512)
        items = next(item for item in summary["shape"] if item["path"] == "$.items")
        self.assertTrue(items["truncated"])

    def test_samples_large_dictionary_keys_before_sorting_or_hashing(self):
        payload = {"hook_type": "Stop"}
        payload.update({f"dynamic-key-{index}": index for index in range(10_000)})
        late_key = "dynamic-key-9999"
        late_label = (
            "key_"
            + hashlib.sha256(late_key.encode("utf-8")).hexdigest()[:16]
        )

        summary = summarize_event(
            payload,
            received_at=datetime(2026, 7, 26, 8, 3, tzinfo=timezone.utc),
        )
        serialized = json.dumps(summary, ensure_ascii=False)

        self.assertLessEqual(len(summary["shape"]), 512)
        self.assertLessEqual(len(summary["top_level_keys"]), 128)
        self.assertTrue(summary["top_level_keys_truncated"])
        self.assertNotIn(late_key, serialized)
        self.assertNotIn(late_label, serialized)

    def test_prioritizes_identity_keys_after_large_ordinary_prefix(self):
        payload = {
            f"dynamic-key-{index}": index
            for index in range(10_000)
        }
        payload["thread_id"] = "late-stable-thread-secret"

        summary = summarize_event(
            payload,
            received_at=datetime(2026, 7, 26, 8, 3, tzinfo=timezone.utc),
        )
        serialized = json.dumps(summary, ensure_ascii=False)
        identities = {
            item["path"]: item for item in summary["identity_candidates"]
        }

        self.assertIn("$.thread_id", identities)
        self.assertRegex(
            identities["$.thread_id"]["fingerprint"],
            r"^[0-9a-f]{16}$",
        )
        self.assertIn("thread_id", summary["top_level_keys"])
        self.assertNotIn("late-stable-thread-secret", serialized)

    def test_hashes_unknown_keys_and_redacts_sensitive_alias_subtrees(self):
        payload = {
            "hook_type": "Stop",
            "customer-private-key-name": "private scalar",
            "tool_input": {
                "dynamic-secret-name": {
                    "nested": "tool input secret",
                }
            },
            "tool_output": {"nested": "tool output secret"},
            "prompt_text": {"nested": "prompt secret"},
            "command_args": {"nested": "command secret"},
        }

        first = summarize_event(
            payload,
            received_at=datetime(2026, 7, 26, 8, 4, tzinfo=timezone.utc),
        )
        second = summarize_event(
            payload,
            received_at=datetime(2026, 7, 26, 8, 4, tzinfo=timezone.utc),
        )
        serialized = json.dumps(first, ensure_ascii=False)

        self.assertNotIn("customer-private-key-name", serialized)
        self.assertNotIn("dynamic-secret-name", serialized)
        self.assertNotIn("private scalar", serialized)
        self.assertNotIn("tool input secret", serialized)
        self.assertNotIn("tool output secret", serialized)
        self.assertNotIn("prompt secret", serialized)
        self.assertNotIn("command secret", serialized)
        self.assertEqual(first["top_level_keys"], second["top_level_keys"])
        for alias in ("tool_input", "tool_output", "prompt_text", "command_args"):
            marker = next(
                item for item in first["shape"] if item["path"] == f"$.{alias}"
            )
            self.assertTrue(marker["redacted"])
            self.assertFalse(
                any(item["path"].startswith(f"$.{alias}.") for item in first["shape"])
            )

    def test_ignores_container_identity_values(self):
        summary = summarize_event(
            {
                "hook_type": "Stop",
                "thread_id": {"nested": "secret"},
                "session_id": ["secret"],
            },
            received_at=datetime(2026, 7, 26, 8, 5, tzinfo=timezone.utc),
        )
        serialized = json.dumps(summary, ensure_ascii=False)

        self.assertEqual(summary["identity_candidates"], [])
        self.assertNotIn("secret", serialized)


if __name__ == "__main__":
    unittest.main()
