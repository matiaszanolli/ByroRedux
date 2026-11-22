"""Contract checks for the A6 reference environment (no GPU needed).

The six 90° `sky-env` captures are resampled into the lat-long map Mitsuba's
`envmap` reads (RLT-REF-012). These tests pin the lat-long convention
measured on Mitsuba 3.9.1 and check the resampling against a smooth sky
known in closed form.
"""
import sys
import unittest
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent))
import gates


def sky(d: np.ndarray) -> np.ndarray:
    """A smooth, direction-dependent test radiance."""
    return np.stack([0.5 + 0.5 * d[..., 0], 1.0 + d[..., 1] ** 2, 0.3 + 0.2 * d[..., 2]], axis=-1)


def pinhole(forward, up, size: int) -> np.ndarray:
    """Per-pixel ray directions of a square 90° pinhole camera (pixel y down)."""
    forward = np.asarray(forward, float)
    right = np.cross(forward, up)
    right /= np.linalg.norm(right)
    down = np.cross(forward, right)
    s = (np.arange(size) + 0.5) / size * 2.0 - 1.0
    x, y = np.meshgrid(s, s)
    d = forward + x[..., None] * right + y[..., None] * down
    return d / np.linalg.norm(d, axis=-1, keepdims=True)


class LatLongConvention(unittest.TestCase):
    def test_matches_mitsuba_3_9_1_texel_directions(self):
        # Measured with a single bright texel in a 64 x 32 envmap (3.9.1):
        # u = 0 faces -Z, 0.25 faces +X, 0.5 faces +Z, 0.75 faces -X.
        d = gates.latlong_directions(64, 32)
        self.assertGreater(d[16, 0] @ np.array([0.0, 0.0, -1.0]), 0.99)
        self.assertGreater(d[16, 16] @ np.array([1.0, 0.0, 0.0]), 0.99)
        self.assertGreater(d[16, 32] @ np.array([0.0, 0.0, 1.0]), 0.99)
        self.assertGreater(d[16, 48] @ np.array([-1.0, 0.0, 0.0]), 0.99)
        self.assertGreater(d[0, 0, 1], 0.99)    # top row looks up
        self.assertLess(d[-1, 0, 1], -0.99)     # bottom row looks down


class Assembly(unittest.TestCase):
    def test_six_views_reproduce_a_smooth_sky(self):
        size = 96
        faces = [([1, 0, 0], [0, 1, 0]), ([-1, 0, 0], [0, 1, 0]), ([0, 1, 0], [0, 0, -1]),
                 ([0, -1, 0], [0, 0, 1]), ([0, 0, 1], [0, 1, 0]), ([0, 0, -1], [0, 1, 0])]
        views = []
        for forward, up in faces:
            dirs = pinhole(forward, up, size)
            views.append(gates.PinholeView.fit(dirs, sky(dirs)))
        env = gates.assemble_sky_env(views, 256, 128)
        expected = sky(gates.latlong_directions(256, 128))
        err = np.abs(env - expected).max()
        self.assertLess(err, 2e-3, f"max error {err}")

    def test_fit_refuses_a_non_pinhole_view(self):
        dirs = pinhole([0, 0, -1], [0, 1, 0], 32)
        dirs[0, 0] = np.array([1.0, 0.0, 0.0])
        with self.assertRaises(SystemExit):
            gates.PinholeView.fit(dirs, np.zeros((32, 32, 3)))


class EnvBrdfTable(unittest.TestCase):
    def test_checked_in_table_is_the_generators_output(self):
        import importlib.util
        root = Path(__file__).resolve().parents[2]
        spec = importlib.util.spec_from_file_location("gen_env_brdf_lut", root / "scripts/gen_env_brdf_lut.py")
        gen = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(gen)
        glsl, rust = gen.render()
        hint = "regenerate with python3 scripts/gen_env_brdf_lut.py"
        self.assertEqual(glsl, (root / "crates/renderer/shaders/include/env_brdf_lut.glsl").read_text(), hint)
        self.assertEqual(rust, (root / "crates/renderer/src/vulkan/scene_buffer/env_brdf_lut.rs").read_text(), hint)


if __name__ == "__main__":
    unittest.main()
