"""Contract checks for the RCG coverage gate (RCG-VAL-002, RCG-VAL-004)."""
import sys
import unittest
from pathlib import Path
from unittest.mock import patch

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent))
import gates


def image(rows):
    """A (1, n, 3) image from a list of grey values."""
    return np.array([[[v, v, v] for v in rows]], dtype=float)


class CoverageTests(unittest.TestCase):
    def test_coverage_counts_traced_pixels_over_the_eligible_ones_only(self):
        eligible = image([1, 1, 1, 1, 0, 0])  # four eligible pixels, two not (sky, glass)
        traced = image([0.2, 0.0, 0.5, 0.0, 0.9, 0.9])  # the last two are outside the mask
        self.assertEqual(gates.coverage_from_views(traced, eligible), (0.5, 4, 2))

    def test_a_pixel_traced_in_one_channel_counts(self):
        eligible = image([1])
        traced = np.array([[[0.0, 0.0, 0.01]]])
        self.assertEqual(gates.coverage_from_views(traced, eligible)[0], 1.0)

    def test_an_empty_mask_is_zero_coverage_not_full(self):
        self.assertEqual(gates.coverage_from_views(image([1, 1]), image([0, 0])), (0.0, 0, 0))

    def test_the_threshold_is_the_prd_q6_figure(self):
        self.assertEqual(gates.COVERAGE_THRESHOLD, 0.99)
        self.assertEqual(gates.COVERAGE_TIERS, (0, 1, 2, 3))

    def test_the_manifest_names_the_five_acceptance_scenes_with_poses(self):
        entries = gates.read_coverage_manifest()
        scenes = {e["scene"] for e in entries}
        self.assertTrue({"prospector", "whiterun", "medtek", "dugout"} <= scenes)
        self.assertTrue(any(s.startswith("fallons") for s in scenes))
        for e in entries:
            for key in ("camera_pos", "camera_forward"):
                parts = [float(x) for x in e[key].split(",")]
                self.assertEqual(len(parts), 3, f"{e['scene']} {key}")
            if e["launch"] != "bench":
                self.assertNotEqual(e["data_dir"], "-", f"{e['scene']} needs a data_dir")

    def test_bench_scenes_resolve_to_a_directory_and_load_arguments(self):
        entry = next(e for e in gates.read_coverage_manifest() if e["scene"] == "prospector")
        workdir, load_args = gates.coverage_scene_spec(entry)
        self.assertEqual(workdir, gates.GAMES_ROOT / "Fallout New Vegas" / "Data")
        self.assertIn("--esm", load_args)

    def test_absent_data_yields_skip_rows_and_never_a_pass(self):
        with patch.object(gates, "GAMES_ROOT", Path("/nonexistent/rcg-games-root")):
            rows = gates.gate_coverage(engine=None, ctx=None)
        self.assertTrue(rows)
        self.assertEqual({row.passed for row, _ in rows}, {"skip"})
        self.assertEqual(gates.EXIT_SKIP, 77)

    def test_the_gate_is_wired_into_the_cli(self):
        source = Path(gates.__file__).read_text()
        self.assertIn('"RCG-COV" in gates', source)
        self.assertIn("return EXIT_SKIP", source)


if __name__ == "__main__":
    unittest.main()
