"""Native probes must never parse a log record still being written."""

import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import Mock, patch

from rich_chat_native_input import last_record
from rich_chat_resize import capture_resize


class NativeRecordTests(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.log = Path(directory.name) / "native.log"
        self.prefix = "RICH CHAT INPUT SIZE "

    def test_partial_record_is_ignored_until_its_newline_arrives(self):
        complete = self.prefix + json.dumps({"rows": 8}) + "\n"
        pending = self.prefix + json.dumps({"rows": 13})
        self.log.write_text(complete + pending[:-3])
        self.assertEqual(last_record(self.log, self.prefix), {"rows": 8})
        self.log.write_text(complete + pending)
        self.assertEqual(last_record(self.log, self.prefix), {"rows": 8})
        with self.log.open("a") as output:
            output.write("\n")
        self.assertEqual(last_record(self.log, self.prefix), {"rows": 13})

    def test_no_completed_matching_record_returns_none(self):
        self.log.write_bytes(b"unrelated output\n" + self.prefix.encode() + b'{"text":"\xc3')
        self.assertIsNone(last_record(self.log, self.prefix))

    def test_malformed_complete_record_is_not_hidden(self):
        self.log.write_text(self.prefix + "{broken}\n")
        with self.assertRaises(json.JSONDecodeError):
            last_record(self.log, self.prefix)

    def test_delayed_drag_events_cannot_supply_the_next_grip_position(self):
        initial = dict(mode="Auto", input_rows=8, conversation_rows=30,
                       divider=[24, 700], pointer=[0, 0],
                       viewport=dict(x=0, y=0, width=100, height=900))
        state = initial.copy()
        queued = []
        starts = []

        def record():
            nonlocal state
            if queued:
                state = queued.pop(0)
            return state

        def drag(start, end):
            starts.append(start)
            if len(starts) == 1:
                self.assertEqual(start, initial["divider"])
                partial = dict(mode="Rows(9)", input_rows=9, conversation_rows=29,
                               divider=[24, 680], pointer=[24, 680])
                final = dict(mode="Rows(14)", input_rows=14, conversation_rows=24,
                             divider=[24, 580], pointer=end)
            else:
                # Using the first intermediate height would miss this grip.
                self.assertEqual(start, [24, 580])
                partial = dict(mode="Rows(13)", input_rows=13, conversation_rows=25,
                               divider=[24, 600], pointer=[24, 610])
                final = dict(mode="Rows(11)", input_rows=11, conversation_rows=27,
                             divider=[24, 640], pointer=end)
            queued.extend([state | partial, state | final])

        def motion(point):
            queued.append(state | {"pointer": point})

        def reset(point):
            self.assertEqual(point, [24, 640])
            queued.append(initial | {"pointer": point})

        app = Mock()
        app.poll.return_value = None
        screenshot = Mock()
        with patch("rich_chat_resize.time.sleep"):
            capture_resize(self.log.parent, app, record, drag, motion, reset, screenshot)
        self.assertEqual(len(starts), 2)
        self.assertEqual(screenshot.call_count, 3)
        report = json.loads((self.log.parent / "input-resize.json").read_text())
        self.assertEqual(report["status"], "passed")
        self.assertEqual(report["checks"][0]["state"]["input_rows"], 14)
        self.assertEqual(report["checks"][1]["state"]["input_rows"], 11)
