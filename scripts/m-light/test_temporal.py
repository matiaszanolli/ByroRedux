"""Contract checks for the temporal capture harness (no GPU needed)."""
import tempfile
import sys
import csv
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent))
import gates


class FakeEngine:
    def __init__(self, out):
        self.out = Path(out)
        self.captures = []
        self.sequences = []

    def capture(self, name, rung, view, seed, tier, **kwargs):
        self.captures.append((name, view, seed, kwargs.get("debug_flags", "0")))
        return name

    def sequence(self, name, rung, tier, frames, bench_frames, extra, **kwargs):
        self.sequences.append((name, list(frames), kwargs.get("view_mode", "final"), kwargs.get("seed", 0),
                               kwargs.get("debug_flags", "0")))
        return {f: (f"{name}_{f}", "") for f in frames}


class TemporalHarnessTests(unittest.TestCase):
    def run_gate(self, gate, seed=0, debug_flags="0"):
        with tempfile.TemporaryDirectory() as directory:
            engine = FakeEngine(directory)
            image = np.ones((4, 4))
            with patch.object(gates, "region_masks", return_value={"pen_floor": image.astype(bool)}), \
                 patch.object(gates, "load_exr", return_value=image), \
                 patch.object(gates, "luminance", side_effect=lambda x: x), \
                 patch.object(gates, "erode", side_effect=lambda x, _: x), \
                 patch.object(gates, "temporal_error", return_value=0.1):
                rows = gates.gate_temporal(engine, None, SimpleNamespace(tier=3), 0, {gate}, seed, debug_flags)
            return engine, rows

    def test_a3_uses_direct_light_and_age_matched_post_stop_frames(self):
        engine, rows = self.run_gate("A3", seed=7)
        still = next(x for x in engine.sequences if x[0].endswith("_still"))
        self.assertEqual(still[1:], ([1, 4, 16], "direct_only", 7, "0"))
        self.assertTrue(all(view == "direct_only" and seed == 7 for _, _, view, seed, _ in engine.sequences))
        self.assertTrue(all(view == "direct_only" and seed == 7 for name, view, seed, _ in engine.captures if "parked" in name))
        self.assertTrue(all(row.temporal_seed == 7 for row, _ in rows))
        gates.apply_thresholds(rows, 3, gates.load_thresholds())
        selected = [row for row, _ in rows if row.case == "penumbra-direct-pan"]
        self.assertEqual(len(selected), 2)
        self.assertTrue(all(row.passed == "report" for row in selected))

    def test_moving_blocker_remains_on_final(self):
        engine, _ = self.run_gate("A14")
        still = next(x for x in engine.sequences if x[0].endswith("_still"))
        stop = gates.TEMPORAL_MOTION - 1
        self.assertEqual(still[1:], ([stop + k for k in (1, 4, 16)], "final", 0, "0"))
        self.assertTrue(all(view == "final" for _, _, view, _, _ in engine.sequences))

    def test_gamma_comparison_keeps_a_final_image_baseline(self):
        engine, _ = self.run_gate("SHD-006")
        names = [name for name, _, _, _, _ in engine.sequences]
        self.assertTrue(any(name.startswith("SHD-006-BASE_") for name in names))
        self.assertTrue(any(name.startswith("SHD-006_") for name in names))
        self.assertTrue(all(view == "final" for _, _, view, _, _ in engine.sequences))

    def test_debug_mask_reaches_every_capture(self):
        engine, _ = self.run_gate("A3", debug_flags="0x40000")
        self.assertTrue(all(flags == "0x40000" for _, _, _, flags in engine.captures))
        self.assertTrue(all(flags == "0x40000" for _, _, _, _, flags in engine.sequences))

    def test_manifest_records_the_temporal_seed_and_no_offline_reference(self):
        _, rows = self.run_gate("A3", seed=7)
        ctx = gates.Context("revision", "gpu", 3, 64, "0,1", 1024, "0,1")
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "manifest.tsv"
            gates.write_manifest(path, rows, ctx)
            with path.open() as stream:
                record = next(csv.DictReader(stream, delimiter="\t"))
        self.assertEqual(record["engine_frames"], "1")
        self.assertEqual(record["engine_seeds"], "7")
        self.assertEqual(record["reference_spp"], "")
        self.assertEqual(record["reference_seeds"], "")

    def test_a3_floor_compares_excess_at_the_same_frame_across_seeds(self):
        _, primary = self.run_gate("A3", seed=0)
        _, paired = self.run_gate("A3", seed=1)
        for row, _ in paired:
            if row.region == "frame_32":
                row.value += 0.04
                row.floor_engine += 0.03
            if row.region == "frame_48":
                row.value += 0.02
        gates.measure_a3_seed_floor(primary, paired)
        motion, settle = [row for row, _ in primary if row.case == "penumbra-direct-pan"]
        self.assertAlmostEqual(motion.floor_engine, 0.02)
        self.assertAlmostEqual(motion.value, 0.02)
        self.assertEqual(motion.temporal_floor_seed, 1)
        self.assertEqual(settle.floor_engine, 0.0)
        gates.apply_thresholds(primary, 3, gates.load_thresholds())
        self.assertEqual(motion.passed, "no")
        self.assertEqual(settle.passed, "yes")


if __name__ == "__main__":
    unittest.main()
