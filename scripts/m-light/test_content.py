"""Contract checks for the A11 content comparison."""
import io
import sys
import unittest
from contextlib import redirect_stdout
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent))
import content


class ContentGateTests(unittest.TestCase):
    def compare(self, anchor, capture, declarations=()):
        args = SimpleNamespace(anchors=Path("anchors"), scene="test",
                               capture=Path("candidate.exr"), declarations=None, margin=0.01)
        with (patch.object(content, "luminance", side_effect=[anchor, anchor, capture]),
              patch.object(content, "load_declarations", return_value=declarations),
              redirect_stdout(io.StringIO())):
            return content.compare(args)

    def test_detects_a_change_in_the_partial_bottom_tile(self):
        anchor = np.ones((33, 32))
        capture = anchor.copy()
        capture[-1, :] *= 1.2
        self.assertEqual(self.compare(anchor, capture), 1)

    def test_declaration_caps_mean_and_p95_change(self):
        anchor = np.ones((32, 32))
        declaration = content.Declaration("test", 0, 0, 0, 0, "brighter", 0.04, 0.0, "")
        self.assertEqual(self.compare(anchor, anchor * 1.03, [declaration]), 0)
        hotspot = anchor.copy()
        hotspot[:2, :] *= 1.5
        self.assertEqual(self.compare(anchor, hotspot, [declaration]), 1)


if __name__ == "__main__":
    unittest.main()
