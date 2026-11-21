#!/usr/bin/env python3
"""M-LIGHT reference gates (docs/prd/reference-lighting.md).

Runs the engine's Cornell reference rungs, exports each scene to Mitsuba 3,
renders the converged reference (cached, RLT-REF-015), and computes the
Cornell gates on linear captures (RLT-REF-007):

  A2   penumbra width             (RLT-SHD-001; width undecided, reported)
  A4   filter bands and thin rod  (RLT-SHD-004; bands gated by the threshold table, rod reported)
  A3   penumbra under camera pan  (RLT-SHD-002; excess over the hold control, gated by the threshold table)
  A5   contact bands              (RLT-CON-001…008; gated by the threshold table at tier 3; the plank also on a flat floor, reported)
  A14  moving blocker             (RLT-SHD-010; excess over the hold control from frame 80, gated by the threshold table)
  A6   sky IBL energy             (RLT-IBL-001…004, RLT-REF-021; probe-only sky rung, report until the L3 budgets)
  A7   GI deficit                 (RLT-GI-002; report until Q5's cache)
  A8   traced share free of SSAO  (RLT-GI-004)
  A15  calibration and materials  (RLT-REF-010, RLT-REF-014)
  A16  closure                    (RLT-REF-011, RLT-REF-018, RLT-REF-020)
  A18  apparatus cost             (RLT-REF-016)
  RCG-COV  traced-share coverage  (RCG-VAL-002, -004; radiance-cache-gi-coverage.md)
  RCG-FLAT GI cost flatness       (RCG-PERF-004, RCG-VAL-004; the receding-plane lab, no game data)

Every metric is written to manifest.tsv with its noise floor on both sides
(RLT-REF-004, RLT-REF-006): each side is captured twice with different
sampling seeds (`--rt-test-seed-offset` on the engine, the sampler seed on
Mitsuba) and the combined floor is the root-sum-square of the two sides.
Rows named in docs/audits/MLIGHT_thresholds.tsv are judged against their
frozen floor, budget and drift margin (RLT-REF-006, Q6); every other row is
`report`. No factor is ever applied to an image; emitters are scaled at export
(k_exp, docs/engine/path-tracing.md §2.1).

Needs a Python with Mitsuba 3 and NumPy (a developer tool, never an engine
dependency). `scripts/m-light-gates.sh` picks the interpreter.
"""

from __future__ import annotations

import argparse
import hashlib
import math
import os
import re
import shlex
import shutil
import subprocess
import sys
import tempfile
import time
import xml.etree.ElementTree as ET
from dataclasses import dataclass, field
from pathlib import Path

import numpy as np
import mitsuba as mi

REPO = Path(__file__).resolve().parents[2]
LUMA = np.array([0.2126, 0.7152, 0.0722])

# Engine captures pass through RGBA16F targets (the main pass's direct
# attachment, composite's scene image). That storage error is deterministic,
# so no seed pair can see it: at these magnitudes one f16 ULP is at most
# 2^-10 of the value, and emulating round-toward-zero on the reference
# accounts for about half of the measured 0.05-0.07 % shortfall. Ratio
# checks allow it on top of the combined floor (reference-lighting rev 11).
F16_REPRESENTATION = 2.0 ** -10

# Region erosion in pixels: drops the partial-coverage pixels at every shape
# edge, where the engine shades the pixel centre and Mitsuba's one-pixel box
# filter integrates the coverage.
ERODE_PX = 3

# Engine world units per metre (byroredux_core::lighting::BETHESDA_UNITS_PER_METER).
# The exporter writes the scene in metres (PTX-004); rung measurements stay in
# world units, so hit positions are scaled back before they meet them.
WORLD_UNITS_PER_METRE = 70.0

MANIFEST_COLUMNS = [
    "gate", "case", "region", "metric", "value", "floor_engine", "floor_reference",
    "floor_combined", "frozen_floor", "threshold", "pass", "tier", "engine_frames",
    "engine_seeds", "reference_spp", "reference_seeds", "k_exp", "reference_cache_key",
    "revision", "gpu",
]


def log(message: str) -> None:
    print(f"m-light: {message}", flush=True)


# ── Mitsuba ──────────────────────────────────────────────────────────────


def pick_variant() -> str:
    for variant in ("cuda_ad_rgb", "llvm_ad_rgb", "scalar_rgb"):
        if variant in mi.variants():
            try:
                mi.set_variant(variant)
                mi.load_dict({"type": "scene"})
                return variant
            except Exception:  # noqa: BLE001 - try the next variant
                continue
    raise SystemExit("m-light: no usable Mitsuba RGB variant")


def scene_digest(scene_dir: Path) -> str:
    """Hash of the exporter's output: scene.xml plus every PLY (PTX-008)."""
    h = hashlib.sha256()
    h.update((scene_dir / "scene.xml").read_bytes())
    for ply in sorted((scene_dir / "meshes").glob("*.ply")):
        h.update(ply.name.encode())
        h.update(ply.read_bytes())
    # The reference environment (RLT-REF-012) and any baked texture.
    for exr in sorted(scene_dir.rglob("*.exr")):
        h.update(str(exr.relative_to(scene_dir)).encode())
        h.update(exr.read_bytes())
    return h.hexdigest()


MAX_SPP_PER_RENDER = 2048  # 1280x720 at 16384 spp overflows the CUDA wavefront; 1024 is proven

RECIPES = {
    # PRD §3.4 in Mitsuba terms (docs/engine/path-tracing.md §2.3).
    "direct": {"max_depth": 2},
    "total": {"max_depth": -1},
}


@dataclass
class ReferenceStore:
    cache: Path
    variant: str
    timings: dict = field(default_factory=lambda: {"rendered": 0, "cached": 0, "seconds": 0.0})

    def key(self, scene_dir: Path, recipe: str, spp: int, seed: int) -> str:
        h = hashlib.sha256()
        for part in (scene_digest(scene_dir), mi.__version__, self.variant, recipe, str(spp), str(seed)):
            h.update(part.encode())
            h.update(b"\0")
        return h.hexdigest()[:24]

    def render(self, scene_dir: Path, recipe: str, spp: int, seed: int) -> tuple[np.ndarray, str]:
        key = self.key(scene_dir, recipe, spp, seed)
        path = self.cache / f"{key}.exr"
        if path.exists():
            self.timings["cached"] += 1
            return load_exr(path), key
        t0 = time.monotonic()
        scene = mi.load_file(str(scene_dir / "scene.xml"), **RECIPES[recipe])
        if spp <= MAX_SPP_PER_RENDER:
            image = mi.render(scene, spp=spp, seed=seed)
        else:
            # One render allocates width x height x spp lanes; past a few
            # thousand samples the CUDA wavefront overflows. Average
            # independent chunks, each on its own seed.
            chunks = -(-spp // MAX_SPP_PER_RENDER)
            image = sum(np.array(mi.render(scene, spp=MAX_SPP_PER_RENDER, seed=seed * 100003 + i))
                        for i in range(chunks)) / chunks
        bitmap = mi.Bitmap(image)
        self.cache.mkdir(parents=True, exist_ok=True)
        bitmap.write(str(path))
        self.timings["rendered"] += 1
        self.timings["seconds"] += time.monotonic() - t0
        return load_exr(path), key


def load_exr(path: Path) -> np.ndarray:
    array = np.array(mi.Bitmap(str(path)), dtype=np.float64)
    return array[..., :3] if array.ndim == 3 else array[..., None]


def shape_reflectance(scene_dir: Path) -> dict[str, np.ndarray]:
    """Diffuse reflectance per shape id, read from the exported XML."""
    out = {}
    root = ET.parse(scene_dir / "scene.xml").getroot()
    for shape in root.findall("shape"):
        bsdf = shape.find("bsdf")
        if bsdf is None or bsdf.get("type") != "diffuse":
            continue
        rgb = bsdf.find("rgb[@name='reflectance']")
        value, name = (rgb.get("value") if rgb is not None else None), shape.get("id")
        if value is not None and name is not None:
            out[name] = np.array([float(v) for v in value.split(",")])
    return out


def pixel_hits(scene_dir: Path) -> tuple[dict[str, np.ndarray], np.ndarray]:
    """Per-shape pixel masks (not eroded) and the hit position per pixel.

    One camera ray per pixel centre — the point the engine shades — is
    intersected with the exported scene and the hit shape compared with each
    shape. (Mitsuba's `shape_index` AOV is not used: its indices follow
    neither the XML order nor `Scene.shapes()`.) Positions are engine world
    units, as the rung measurements are (the exported scene is in metres,
    PTX-004), shape (height, width, 3).
    """
    import drjit as dr

    # `optimize=False`: Mitsuba otherwise merges shapes that share a BSDF
    # value into one unnamed shape, and their regions would vanish.
    scene = mi.load_file(str(scene_dir / "scene.xml"), optimize=False)
    sensor = scene.sensors()[0]
    width, height = sensor.film().size()
    index = dr.arange(mi.UInt32, width * height)
    uv = mi.Point2f((mi.Float(index % width) + 0.5) / width,
                    (mi.Float(index // width) + 0.5) / height)
    rays, _ = sensor.sample_ray(0.0, 0.0, uv, mi.Point2f(0.5, 0.5))
    si = scene.ray_intersect(rays)
    masks = {shape.id(): np.array(si.shape == mi.ShapePtr(shape)).reshape(height, width)
             for shape in scene.shapes()}
    positions = np.array(si.p).T.reshape(height, width, 3) * WORLD_UNITS_PER_METRE
    return masks, positions


def region_masks(scene_dir: Path, erode_px: int = ERODE_PX) -> dict[str, np.ndarray]:
    """Per-shape pixel masks, eroded by `erode_px` (see `pixel_hits`)."""
    masks, _ = pixel_hits(scene_dir)
    return {name: erode(mask, erode_px) for name, mask in masks.items()}


def erode(mask: np.ndarray, radius: int) -> np.ndarray:
    out = mask.copy()
    for _ in range(radius):
        shrunk = out.copy()
        shrunk[1:, :] &= out[:-1, :]
        shrunk[:-1, :] &= out[1:, :]
        shrunk[:, 1:] &= out[:, :-1]
        shrunk[:, :-1] &= out[:, 1:]
        out = shrunk
    return out


def luminance(image: np.ndarray) -> np.ndarray:
    return image @ LUMA


# ── Engine ───────────────────────────────────────────────────────────────


@dataclass
class Engine:
    binary: Path
    out: Path
    env_base: dict
    frames_warm: int
    capture_frames: int

    def capture(self, name: str, rung: str, view_mode: str, seed: int, tier: int, *,
                extra: list[str] | None = None, debug_flags: str = "0",
                export_dir: Path | None = None, reference_view: int = 0,
                bench_frames: int | None = None, bench_mode: str = "renderer-static") -> Path:
        exr = self.out / "engine" / f"{name}_s{seed}.exr"
        exr.parent.mkdir(parents=True, exist_ok=True)
        args = [
            str(self.binary), "--cornell-reference", rung,
            "--cornell-reference-view", str(reference_view),
            "--render-debug-mode", view_mode,
            "--bench-frames", str(bench_frames or self.frames_warm), "--bench-mode", bench_mode,
            "--rt-test-ray-quality-tier", str(tier), "--rt-test-seed-offset", str(seed),
            # RLT-REF-009: reference rungs carry no medium.
            "--rt-test-no-dust-floor",
            "--linear-capture", str(exr), "--linear-capture-frames", str(self.capture_frames),
        ] + (extra or [])
        if export_dir is not None:
            args += ["--export-mitsuba", str(export_dir)]
        env = dict(self.env_base, BYROREDUX_RENDER_DEBUG=debug_flags, RUST_LOG="error")
        log_path = exr.with_suffix(".log")
        with open(log_path, "w") as log_file:
            subprocess.run(args, env=env, stdout=log_file, stderr=subprocess.STDOUT, timeout=900)
        text = log_path.read_text(errors="replace")
        if "linear-capture: wrote" not in text or "nonfinite=0)" not in text:
            raise SystemExit(f"m-light: engine capture {name} failed; see {log_path}")
        if export_dir is not None and "export-mitsuba: wrote" not in text:
            raise SystemExit(f"m-light: export for {name} failed; see {log_path}")
        return exr

    def sequence(self, name: str, rung: str, tier: int, frames: list[int], bench_frames: int,
                 extra: list[str], view_mode: str = "final", seed: int = 0,
                 debug_flags: str = "0") -> dict[int, tuple[Path, str]]:
        """Single-frame captures at `frames` of one renderer-stepped run
        (`--linear-capture-at`), each with the camera pose the log names."""
        pattern = self.out / "engine" / f"{name}_s{seed}_f{{frame}}.exr"
        pattern.parent.mkdir(parents=True, exist_ok=True)
        args = [
            str(self.binary), "--cornell-reference", rung, "--render-debug-mode", view_mode,
            "--bench-frames", str(bench_frames), "--bench-mode", "renderer-stepped",
            "--rt-test-ray-quality-tier", str(tier), "--rt-test-seed-offset", str(seed),
            "--rt-test-no-dust-floor",
            "--linear-capture", str(pattern), "--linear-capture-at", ",".join(map(str, frames)),
        ] + extra
        env = dict(self.env_base, BYROREDUX_RENDER_DEBUG=debug_flags, RUST_LOG="error")
        log_path = self.out / "engine" / f"{name}_s{seed}.log"
        with open(log_path, "w") as log_file:
            subprocess.run(args, env=env, stdout=log_file, stderr=subprocess.STDOUT, timeout=900)
        text = log_path.read_text(errors="replace")
        if "--export-mitsuba" in extra and "export-mitsuba: wrote" not in text:
            raise SystemExit(f"m-light: export for {name} failed; see {log_path}")
        poses = {}
        for line in text.splitlines():
            if line.startswith("linear-capture: frame ") and " camera " in line:
                frame = int(line.split()[2])
                poses[frame] = line.split(" camera ", 1)[1]
        out = {}
        for frame in frames:
            path = Path(str(pattern).replace("{frame}", str(frame)))
            if f"wrote {path} " not in text or not path.exists():
                raise SystemExit(f"m-light: sequence {name} missed frame {frame}; see {log_path}")
            out[frame] = (path, poses.get(frame, ""))
        return out


# ── Metrics ──────────────────────────────────────────────────────────────


def region_mean(image: np.ndarray, mask: np.ndarray) -> float:
    if not mask.any():
        raise SystemExit("m-light: an empty region — the camera or the erosion is wrong")
    return float(luminance(image)[mask].mean())


def ratio(engine: np.ndarray, reference: np.ndarray, mask: np.ndarray) -> float:
    return region_mean(engine, mask) / region_mean(reference, mask)


@dataclass
class Row:
    gate: str
    case: str
    region: str
    metric: str
    value: float
    floor_engine: float
    floor_reference: float
    passed: str = ""
    threshold: str = ""
    frozen_floor: str = ""
    # Set where a gate renders its references at a different sample count
    # than the overall run. The manifest records the actual count.
    reference_spp: int | None = None
    temporal_seed: int | None = None
    temporal_floor_seed: int | None = None

    @property
    def floor_combined(self) -> float:
        return math.hypot(self.floor_engine, self.floor_reference)


@dataclass
class Context:
    revision: str
    gpu: str
    tier: int
    engine_frames: int
    engine_seeds: str
    reference_spp: int
    reference_seeds: str
    k_exp: str = "point=pi"


def write_manifest(path: Path, rows: list[tuple[Row, str]], ctx: Context) -> None:
    with open(path, "w") as out:
        out.write("\t".join(MANIFEST_COLUMNS) + "\n")
        for row, key in rows:
            # Temporal sequences use single moving/held frames; their parked
            # references are engine captures, not Mitsuba seeds.
            temporal = row.gate in {"A3", "A14", "SHD-006", "SHD-006-BASE"}
            reference_free = temporal or row.reference_spp == 0
            engine_seeds = "0" if row.reference_spp == 0 else ctx.engine_seeds
            if temporal:
                engine_seeds = str(row.temporal_seed if row.temporal_seed is not None else 0)
                if row.temporal_floor_seed is not None:
                    engine_seeds += f",{row.temporal_floor_seed}"
            k_exp = {"cal-sun": "sun=pi", "penumbra-sun": "sun=pi",
                     "cal-sky": "environment=1"}.get(row.case, ctx.k_exp)
            values = [
                row.gate, row.case, row.region, row.metric, f"{row.value:.6g}",
                f"{row.floor_engine:.3g}", f"{row.floor_reference:.3g}", f"{row.floor_combined:.3g}",
                row.frozen_floor, row.threshold, row.passed, str(ctx.tier), "1" if temporal else str(ctx.engine_frames),
                engine_seeds,
                "" if reference_free else str(row.reference_spp or ctx.reference_spp),
                "" if reference_free else ctx.reference_seeds, k_exp, key,
                ctx.revision, ctx.gpu,
            ]
            out.write("\t".join(values) + "\n")


def unit_ratio_row(gate: str, case: str, region: str, ea, eb, ra, rb, mask) -> Row:
    """A ratio that must be 1 within the combined floor (RLT-REF-010, -014)."""
    value = ratio(ea, ra, mask)
    floor_e = abs(ratio(eb, ra, mask) - value)
    floor_r = abs(ratio(ea, rb, mask) - value)
    row = Row(gate, case, region, "engine_over_reference", value, floor_e, floor_r)
    row.threshold = "|c-1|<=floor_combined+2^-10(f16)"
    row.passed = "yes" if abs(value - 1.0) <= row.floor_combined + F16_REPRESENTATION else "no"
    return row


# ── Gates ────────────────────────────────────────────────────────────────


# Calibration rungs per emitter kind, each from the phase that first uses it
# (A15): the point from Phase 1a, the sun from Phase 3, the sky from Phase 4.
CALIBRATION_RUNGS = ("cal-point", "cal-sun", "cal-sky")


def gate_calibration(engine: Engine, store: ReferenceStore, ctx: Context, spp: int):
    """A15, per emitter kind (RLT-REF-010): one isolated term over the plane."""
    rows = []
    for rung in CALIBRATION_RUNGS:
        rows += calibration_rows(engine, store, ctx, spp, rung)
    return rows


def calibration_rows(engine: Engine, store: ReferenceStore, ctx: Context, spp: int, rung: str):
    rows = []
    export = engine.out / "scenes" / rung
    # The sky plane has unit albedo, so its indirect-only irradiance already
    # matches the reference's outgoing radiance.
    view = "indirect_only" if rung == "cal-sky" else "direct_only"
    name = rung.replace("-", "_") + ("_sky" if rung == "cal-sky" else "_direct")
    native = ["--upscaler", "taa"]
    ea = engine.capture(name, rung, view, 0, ctx.tier, export_dir=export, extra=native)
    eb = engine.capture(name, rung, view, 1, ctx.tier, extra=native)
    ra, key = store.render(export, "direct", spp, 0)
    rb, _ = store.render(export, "direct", spp, 1)
    mask = region_masks(export)["cal_plane"]
    ea, eb = load_exr(ea), load_exr(eb)
    rows.append((unit_ratio_row("A15", rung, "cal_plane", ea, eb, ra, rb, mask), key))
    # Constancy across the plane (inverse-square and cosine laws): the ratio
    # on a 3x3 grid of sub-regions, each its own unit-ratio row.
    ys, xs = np.nonzero(mask)
    y_edges = np.linspace(ys.min(), ys.max() + 1, 4).astype(int)
    x_edges = np.linspace(xs.min(), xs.max() + 1, 4).astype(int)
    for j in range(3):
        for i in range(3):
            sub = np.zeros_like(mask)
            sub[y_edges[j]:y_edges[j + 1], x_edges[i]:x_edges[i + 1]] = True
            sub &= mask
            if sub.sum() < 200:
                continue
            rows.append((unit_ratio_row("A15", rung, f"cal_plane_{j}{i}", ea, eb, ra, rb, sub), key))
    return rows


def gate_materials(engine: Engine, store: ReferenceStore, ctx: Context, spp: int):
    """A15, flat planes (RLT-REF-014): each mapped material at three views."""
    from_rung = None
    rows = []
    for view in (0, 1, 2):
        export = engine.out / "scenes" / f"materials-v{view}"
        ea = engine.capture(f"materials_v{view}_direct", "materials", "direct_only", 0, ctx.tier,
                            export_dir=export, reference_view=view)
        eb = engine.capture(f"materials_v{view}_direct", "materials", "direct_only", 1, ctx.tier,
                            reference_view=view)
        ra, key = store.render(export, "direct", spp, 0)
        rb, _ = store.render(export, "direct", spp, 1)
        masks = region_masks(export)
        ea, eb = load_exr(ea), load_exr(eb)
        from_rung = [n for n in masks if n.startswith("mat_lambert_")]
        for name in from_rung:
            rows.append((unit_ratio_row("A15", f"materials-v{view}", name, ea, eb, ra, rb, masks[name]), key))
    # The normal-mapped plane (RLT-REF-014, from Phase 3) at three light
    # azimuths: the normal-map convention of path-tracing.md §2.2.
    for view in (0, 1, 2):
        export = engine.out / "scenes" / f"normal-plane-v{view}"
        ea = engine.capture(f"normal_plane_v{view}_direct", "normal-plane", "direct_only", 0, ctx.tier,
                            export_dir=export, reference_view=view)
        eb = engine.capture(f"normal_plane_v{view}_direct", "normal-plane", "direct_only", 1, ctx.tier,
                            reference_view=view)
        ra, key = store.render(export, "direct", spp, 0)
        rb, _ = store.render(export, "direct", spp, 1)
        mask = region_masks(export)["nm_plane"]
        rows.append((unit_ratio_row("A15", f"normal-plane-azimuth{view}", "nm_plane", load_exr(ea), load_exr(eb),
                                    ra, rb, mask), key))
    return rows


def truncate_mantissa(x: np.ndarray, bits: int) -> np.ndarray:
    """Round toward zero to a float with `bits` mantissa bits."""
    raw = np.asarray(x, dtype=np.float32).view(np.uint32)
    keep = np.uint32((0xFFFFFFFF << (23 - bits)) & 0xFFFFFFFF)
    return (raw & keep).view(np.float32).astype(np.float64)


def as_b10g11r11(rgb: np.ndarray) -> np.ndarray:
    """What a `B10G11R11_UFLOAT` attachment stores: 6-bit R/G and 5-bit B
    mantissas, truncated (the measured behaviour on the reference GPU)."""
    return np.stack([truncate_mantissa(rgb[..., 0], 6), truncate_mantissa(rgb[..., 1], 6),
                     truncate_mantissa(rgb[..., 2], 5)], axis=-1)


def closure_error(final, fdirect, indirect, albedo_by_region, masks, model_storage=True):
    """Engine closure per region: (final − fd − albedo ⊙ indirect) / final.

    Composite reads the indirect term and the albedo from B10G11R11_UFLOAT
    images (the G-buffer's `raw_indirect` / `albedo`, SVGF's output;
    `gbuffer.rs`), so the prediction stores both the same way; the
    unmodelled variant is what that storage costs the final image.
    """
    out = {}
    for name, albedo in albedo_by_region.items():
        mask = masks.get(name)
        if mask is None or not mask.any() or not albedo.any():
            continue
        if model_storage:
            predicted = fdirect + as_b10g11r11(indirect) * as_b10g11r11(albedo[None, :])[0]
        else:
            predicted = fdirect + indirect * albedo
        f = float(luminance(final)[mask].mean())
        out[name] = (f - float(luminance(predicted)[mask].mean())) / max(f, 1e-9)
    return out


def gate_closure(engine: Engine, store: ReferenceStore, ctx: Context, spp: int):
    """A16 (RLT-REF-011): components sum to the total on both sides."""
    rows = []
    export = engine.out / "scenes" / "lambert-box"
    # SVGF bypassed (RLT-REF-018) and no jitter (RLT-REF-020); the capture is
    # read before TAA, so `final` holds no temporal reconstruction.
    closure = ["--upscaler", "taa", "--rt-test-no-jitter"]
    captures = {}
    for seed in (0, 1):
        for view in ("final", "filtered_direct", "indirect_only"):
            captures[(view, seed)] = load_exr(engine.capture(
                f"closure_{view}", "lambert-box", view, seed, ctx.tier, extra=closure,
                debug_flags="0x20", export_dir=export if (seed, view) == (0, "final") else None))
    masks = region_masks(export)
    albedo = shape_reflectance(export)
    err = [closure_error(captures[("final", s)], captures[("filtered_direct", s)],
                         captures[("indirect_only", s)], albedo, masks) for s in (0, 1)]
    unmodelled = closure_error(captures[("final", 0)], captures[("filtered_direct", 0)],
                               captures[("indirect_only", 0)], albedo, masks, model_storage=False)
    for name, value in err[0].items():
        row = Row("A16", "lambert-box-engine", name, "closure_relative_error", value,
                  abs(err[1][name] - value), 0.0)
        # Same-seed components share their samples, so the floor is the
        # seed-to-seed change of the closure error itself; the f16 capture
        # allowance covers the deterministic storage no seed pair sees.
        row.threshold = "|err|<=floor_combined+2^-10(f16)"
        row.passed = "yes" if abs(value) <= row.floor_combined + F16_REPRESENTATION else "no"
        rows.append((row, ""))
        # What B10G11R11 storage of indirect and albedo costs `final`
        # (negative: darker), as a share of the final luminance.
        rows.append((Row("A16", "lambert-box-storage-report", name, "b10g11r11_final_change",
                         unmodelled[name] - value, 0.0, 0.0, passed="report"), ""))

    # Reference side: total == direct + (total' − direct') with independent seeds.
    total0, key = store.render(export, "total", spp, 0)
    direct0, _ = store.render(export, "direct", spp, 0)
    total1, _ = store.render(export, "total", spp, 1)
    direct1, _ = store.render(export, "direct", spp, 1)
    for name in albedo:
        mask = masks.get(name)
        if mask is None or not mask.any():
            continue
        t0 = region_mean(total0, mask)
        # Per-pixel closure field from independent seeds; Mitsuba samples
        # every pixel independently, so its region mean's standard error
        # is std/sqrt(N) — the floor of this check, tested at 3 sigma.
        field = luminance(total0 - direct0 - (total1 - direct1))[mask]
        value = float(field.mean()) / t0
        floor_r = float(field.std() / math.sqrt(field.size)) / t0
        row = Row("A16", "lambert-box-reference", name, "closure_relative_error", value, 0.0, floor_r)
        row.threshold = "|err|<=3*standard_error"
        row.passed = "yes" if abs(value) <= 3.0 * row.floor_combined else "no"
        rows.append((row, key))

    # SVGF energy change, handed to M54 as a report (RLT-REF-011).
    svgf_on = load_exr(engine.capture("closure_final_svgf_on", "lambert-box", "final", 0, ctx.tier,
                                      extra=closure))
    for name in albedo:
        mask = masks.get(name)
        if mask is None or not mask.any():
            continue
        bypass = region_mean(captures[("final", 0)], mask)
        change = (region_mean(svgf_on, mask) - bypass) / max(bypass, 1e-9)
        rows.append((Row("A16", "lambert-box-svgf-report", name, "svgf_energy_change", change, 0.0, 0.0,
                         passed="report"), ""))
    return rows


L4_REGIONS = ("box_floor", "box_back", "box_ceiling")


def deficit_rows(case: str, ind_a, ind_b, ref_ind_a, ref_ind_b, albedo, masks) -> list[Row]:
    """RLT-GI-002: energy deficit and relative error distribution per region.

    Engine side is `indirect_only` (pre-SVGF, not multiplied by albedo) times
    the region's flat albedo; reference side is total − direct (PRD §3.4).
    """
    rows = []
    for name in L4_REGIONS:
        mask, alb = masks[name], albedo[name]
        e_a, e_b = ind_a * alb, ind_b * alb

        def deficit(e, r):
            return 1.0 - region_mean(e, mask) / region_mean(r, mask)

        def rel_error(e, r):
            # Per pixel |E − R| normalised by the region's mean reference
            # indirect (§2 relative error distribution).
            norm = region_mean(r, mask)
            err = np.abs(luminance(e)[mask] - luminance(r)[mask]) / norm
            return float(err.mean()), float(np.percentile(err, 95))

        d = deficit(e_a, ref_ind_a)
        rows.append(Row("A7", case, name, "energy_deficit", d,
                        abs(deficit(e_b, ref_ind_a) - d), abs(deficit(e_a, ref_ind_b) - d), passed="report"))
        mean_a, p95_a = rel_error(e_a, ref_ind_a)
        mean_b, p95_b = rel_error(e_b, ref_ind_a)
        mean_r, p95_r = rel_error(e_a, ref_ind_b)
        rows.append(Row("A7", case, name, "rel_error_mean", mean_a, abs(mean_b - mean_a), abs(mean_r - mean_a),
                        passed="report"))
        rows.append(Row("A7", case, name, "rel_error_p95", p95_a, abs(p95_b - p95_a), abs(p95_r - p95_a),
                        passed="report"))
    return rows


def gate_gi_deficit(engine: Engine, store: ReferenceStore, ctx: Context, spp: int):
    """A7 (RLT-GI-002, RLT-GI-003 report-only, RLT-GI-006, RLT-GI-010)."""
    rows = []
    export = engine.out / "scenes" / "lambert-box"
    total = [store.render(export, "total", spp, s) for s in (0, 1)] if (export / "scene.xml").exists() else None
    cases = [(f"tier{t}", t, []) for t in (0, 1, 2, 3)]
    cases += [("tier3-no-gi-clamp", 3, ["--rt-test-no-gi-clamp"]),
              ("tier3-no-rt-lod", 3, ["--rt-test-lod-scale", "0.000001"])]
    for case, tier, extra in cases:
        caps = [load_exr(engine.capture(f"gi_{case}_indirect", "lambert-box", "indirect_only", seed, tier,
                                        extra=extra, export_dir=export if total is None else None))
                for seed in (0, 1)]
        if total is None:
            total = [store.render(export, "total", spp, s) for s in (0, 1)]
        direct = [store.render(export, "direct", spp, s) for s in (0, 1)]
        ref_ind = [total[i][0] - direct[i][0] for i in (0, 1)]
        masks, albedo = region_masks(export), shape_reflectance(export)
        rows += [(row, total[0][1]) for row in
                 deficit_rows(case, caps[0], caps[1], ref_ind[0], ref_ind[1], albedo, masks)]
    return rows


def gate_traced_share_ao(engine: Engine, store: ReferenceStore, ctx: Context, spp: int):
    """A8 (RLT-GI-004): the traced share must not be multiplied by SSAO."""
    del store, spp  # engine against itself
    rows = []
    # The box rung's close-up (view 1): SSAO's screen radius shrinks with
    # distance and is inert at the front view's 5-7 m.
    export = engine.out / "scenes" / "lambert-box-closeup"
    needs_export = not (export / "scene.xml").exists()
    on = [load_exr(engine.capture("traced_share_ao_on", "lambert-box", "traced_share", s, ctx.tier,
                                  export_dir=export if needs_export and s == 0 else None,
                                  reference_view=1))
          for s in (0, 1)]
    off = load_exr(engine.capture("traced_share_ao_off", "lambert-box", "traced_share", 0, ctx.tier,
                                  debug_flags="0x800", reference_view=1))
    # SSAO acts within a few pixels of where geometry meets (block bases,
    # room corners). Those are exactly the pixels the other gates' edge
    # erosion removes, so A8 uses un-eroded shapes and all geometry together;
    # a traced-share capture is the engine against itself, so the partial
    # coverage that erosion exists for does not arise.
    masks = region_masks(export, erode_px=0)
    regions = {name: mask for name, mask in masks.items() if not name.endswith("_emitter")}
    regions["all_geometry"] = np.logical_or.reduce(list(regions.values()))
    for name, mask in regions.items():
        if not mask.any() or region_mean(off, mask) <= 1e-6:
            continue
        base = region_mean(off, mask)
        value = (region_mean(on[0], mask) - base) / max(base, 1e-9)
        floor = abs(region_mean(on[1], mask) - region_mean(on[0], mask)) / max(base, 1e-9)
        row = Row("A8", "lambert-box", name, "ssao_relative_change", value, floor, 0.0)
        row.threshold = "|change|<=floor_combined"
        row.passed = "yes" if abs(value) <= row.floor_combined else "no"
        rows.append((row, ""))
    return rows


# ── RCG coverage (RCG-VAL-002, RCG-VAL-004) ──────────────────────────────

GAMES_ROOT = Path(os.environ.get("BYROREDUX_GAMES_ROOT", "/mnt/data/SteamLibrary/steamapps/common"))
COVERAGE_MANIFEST = REPO / "docs" / "audits" / "RCG_coverage_manifest.tsv"
COVERAGE_THRESHOLD = 0.99  # radiance-cache-gi-coverage.md Q6
COVERAGE_TIERS = (0, 1, 2, 3)
EXIT_SKIP = 77  # the repo's SKIP code (docs/smoke-tests/README.md): the data was absent, nothing was judged


def read_coverage_manifest(path: Path = COVERAGE_MANIFEST) -> list[dict[str, str]]:
    lines = [l for l in path.read_text().splitlines() if l and not l.startswith("#")]
    header, *rows = [l.split("\t") for l in lines]
    return [dict(zip(header, row)) for row in rows]


def coverage_scene_spec(entry: dict[str, str]) -> tuple[Path, list[str]]:
    """(working directory, load arguments) of a manifest scene."""
    if entry["launch"] != "bench":
        args = shlex.split(entry["launch"])
        # `--game` looks its profile up in `assets/debug_profiles.toml` relative to the working
        # directory, so those launches run from the repository, not from the Data folder.
        return (REPO if "--game" in args else GAMES_ROOT / entry["data_dir"]), args
    script = ('source "$REPO/scripts/bench-scenes.sh"; run_scene_args "$1"; '
              'printf "%s\\n" "$(scene_dir "$1")"; printf "%s\\0" "${ARGS[@]}"')
    out = subprocess.run(["bash", "-c", script, "bench-scenes", entry["scene"]],
                         env=dict(os.environ, REPO=str(REPO), GAMES_ROOT=str(GAMES_ROOT)),
                         capture_output=True, check=True).stdout
    head, _, blob = out.partition(b"\n")
    return Path(head.decode()), [a.decode() for a in blob.split(b"\0") if a]


def coverage_from_views(traced: np.ndarray, eligible: np.ndarray) -> tuple[float, int, int]:
    """(coverage, eligible pixels, eligible pixels with a traced share): the PRD's definition,
    measured on `RENDER_DEBUG_TRACED_SHARE` over the `gi_eligible` mask. A traced share is non-zero
    when any channel is (the frames of one capture are averaged, so a pixel traced in any of them
    counts)."""
    mask = eligible.min(axis=2) > 0.5
    total = int(mask.sum())
    if total == 0:
        return 0.0, 0, 0
    hit = int((mask & (traced.max(axis=2) > 0.0)).sum())
    return hit / total, total, hit


def gate_coverage(engine: Engine, ctx: Context, only: set[str] | None = None):
    """RCG-VAL-002: coverage per scene and tier against the Q6 threshold. A scene whose data is
    absent yields one `skip` row, never a pass (RCG-VAL-004)."""
    rows = []
    for entry in read_coverage_manifest():
        scene = entry["scene"]
        if only and scene not in only:
            continue
        workdir, load_args = coverage_scene_spec(entry)
        data = GAMES_ROOT / entry["data_dir"] if entry["data_dir"] != "-" else workdir
        if not data.is_dir():
            log(f"RCG-COV {scene}: {data} is absent, skipped")
            row = Row("RCG-COV", scene, "all", "coverage", float("nan"), 0.0, 0.0,
                      passed="skip", threshold=f">={COVERAGE_THRESHOLD}", reference_spp=0)
            rows.append((row, ""))
            continue
        for tier in COVERAGE_TIERS:
            views = {}
            for view in ("traced_share", "gi_eligible"):
                exr = engine.out / "engine" / f"rcg_cov_{scene}_t{tier}_{view}.exr"
                exr.parent.mkdir(parents=True, exist_ok=True)
                args = [str(engine.binary), *load_args,
                        "--upscaler", "taa", "--rt-test-no-jitter", "--rt-test-ray-quality-tier", str(tier),
                        "--camera-pos", entry["camera_pos"], "--camera-forward", entry["camera_forward"],
                        "--render-debug-mode", view, "--bench-frames", "30", "--bench-mode", "renderer-static",
                        "--linear-capture", str(exr), "--linear-capture-frames", str(engine.capture_frames)]
                log_path = exr.with_suffix(".log")
                with tempfile.TemporaryDirectory() as settings_dir, open(log_path, "w") as log_file:
                    env = dict(engine.env_base, RUST_LOG="error", BYRO_CLOUD_SEED="0",
                               BYROREDUX_SETTINGS_PATH=str(Path(settings_dir) / "settings.toml"))
                    subprocess.run(args, cwd=workdir, env=env, stdout=log_file, stderr=subprocess.STDOUT,
                                   timeout=900)
                text = log_path.read_text(errors="replace")
                if "linear-capture: wrote" not in text or "nonfinite=0)" not in text:
                    raise SystemExit(f"m-light: coverage capture {scene} tier {tier} {view} failed; see {log_path}")
                if "profile not found" in text:
                    raise SystemExit(f"m-light: coverage scene {scene} did not load (profile not found); see {log_path}")
                views[view] = load_exr(exr)
            coverage, total, hit = coverage_from_views(views["traced_share"], views["gi_eligible"])
            if total == 0:
                # No eligible pixel is a scene that did not render, never 0 % coverage.
                raise SystemExit(f"m-light: coverage scene {scene} tier {tier} has no eligible pixel; "
                                 f"the scene did not render (see {engine.out / 'engine'})")
            ok = total > 0 and coverage >= COVERAGE_THRESHOLD
            region = f"tier{tier}"
            rows.append((Row("RCG-COV", scene, region, "coverage", coverage, 0.0, 0.0,
                             passed="yes" if ok else "no", threshold=f">={COVERAGE_THRESHOLD}",
                             reference_spp=0), ""))
            rows.append((Row("RCG-COV", scene, region, "eligible_pixels", float(total), 0.0, 0.0,
                             passed="report", reference_spp=0), ""))
            rows.append((Row("RCG-COV", scene, region, "traced_pixels", float(hit), 0.0, 0.0,
                             passed="report", reference_spp=0), ""))
    return rows


# ── RCG flatness (RCG-PERF-004, RCG-VAL-004) ─────────────────────────────

# Camera heights over the receding-plane lab's plane, metres: a x4 series from 2 m to 2 km. The camera
# looks straight down, so every pixel of a frame lies at about its height and the plane fills the frame:
# one bucket is one distance, with the same eligible pixel count in every bucket.
FLATNESS_HEIGHTS_M = (2, 8, 32, 128, 512, 2048)
FLATNESS_RUNS = 5  # runs per bucket and per side (GI on, GI off), interleaved
FLATNESS_BENCH_FRAMES = 300
FLATNESS_TIER = 3
# The spread the cost may show, as (dearest - cheapest) / cheapest over the buckets (radiance-cache-gi-coverage.md §8).
# `None` until Phase 1 publishes the figure (RCG-PERF-004): the gate then reports and never judges.
FLATNESS_BUDGET: float | None = None
FLATNESS_FILL_TOLERANCE = 0.01  # fragments per bucket may differ from the median by this fraction
DBG_DISABLE_GI_RAYS = 0x10000000  # crates/renderer/src/shader_constants_data.rs
WORLD_UNITS_PER_METER_ENGINE = 70.0


def flatness_stats(deltas: list[float], floors: list[float]) -> tuple[float, float]:
    """(spread in ms, spread ratio) of the per-bucket GI cost. The cheapest bucket is taken no lower
    than the largest noise floor, so measurement noise cannot make the ratio spuriously huge; a
    cheapest bucket that is genuinely zero (the footprint gate culls the far pixels) gives `inf`."""
    cheapest = max(min(deltas), max(floors))
    spread = max(deltas) - min(deltas)
    return spread, (spread / cheapest if cheapest > 0 else float("inf"))


def parse_flatness_run(text: str) -> tuple[float, int, int, int]:
    """(main-pass ms, fragments, gi_traced, gi_lod_culled) of one lab run's log."""
    bench = [l for l in text.splitlines() if l.startswith("bench:")]
    main = re.search(r"gpu_main_render=([0-9.]+)", bench[-1]) if bench else None
    sample = [l for l in text.splitlines() if "rt-lod-telemetry:" in l]
    counts = [re.search(rf"{key}=(\d+)", sample[-1]) for key in ("fragments", "gi_traced", "gi_lod_culled")] if sample else []
    if main is None or not counts or not all(counts):
        raise SystemExit("m-light: a flatness run printed no bench line or no rt-lod-telemetry line")
    return (float(main.group(1)), *(int(c.group(1)) for c in counts))


def gate_flatness(engine: Engine, ctx: Context):
    """RCG-PERF-004: the main-pass cost the GI rays add, per distance bucket of the receding-plane lab."""
    results: dict[int, dict[str, list[tuple[float, int, int, int]]]] = {
        h: {"on": [], "off": []} for h in FLATNESS_HEIGHTS_M}
    for run in range(FLATNESS_RUNS):
        for height in FLATNESS_HEIGHTS_M:
            for side, flags in (("on", 0), ("off", DBG_DISABLE_GI_RAYS)):
                log_path = engine.out / "engine" / f"rcg_flat_h{height}_{side}_r{run}.log"
                log_path.parent.mkdir(parents=True, exist_ok=True)
                args = [str(engine.binary), "--recede-lab",
                        "--camera-pos", f"0,{height * WORLD_UNITS_PER_METER_ENGINE:g},0",
                        "--camera-forward", "0,-1,-0.05",
                        "--rt-test-ray-quality-tier", str(FLATNESS_TIER), "--rt-test-lod-telemetry",
                        "--bench-frames", str(FLATNESS_BENCH_FRAMES), "--bench-mode", "renderer-static"]
                env = dict(engine.env_base, BYROREDUX_RENDER_DEBUG=hex(flags),
                           RUST_LOG="warn,byroredux_renderer::vulkan::context::sync_and_acquire_frame=info")
                with tempfile.TemporaryDirectory() as settings_dir, open(log_path, "w") as log_file:
                    env["BYROREDUX_SETTINGS_PATH"] = str(Path(settings_dir) / "settings.toml")
                    subprocess.run(args, cwd=REPO, env=env, stdout=log_file, stderr=subprocess.STDOUT, timeout=300)
                results[height][side].append(parse_flatness_run(log_path.read_text(errors="replace")))

    # The plane must fill the frame in every bucket, or the buckets do not hold the same pixels.
    fragments = sorted(r[1] for h in results.values() for r in h["on"])
    median_fragments = fragments[len(fragments) // 2]
    rows, deltas, floors = [], [], []
    for height, sides in results.items():
        for run in sides["on"]:
            if abs(run[1] - median_fragments) > FLATNESS_FILL_TOLERANCE * median_fragments:
                raise SystemExit(f"m-light: flatness bucket {height} m has {run[1]} fragments against a median of "
                                 f"{median_fragments}: the plane does not fill the frame")
        on = sorted(r[0] for r in sides["on"])
        off = sorted(r[0] for r in sides["off"])
        delta = on[len(on) // 2] - off[len(off) // 2]
        floor = math.hypot(on[-1] - on[0], off[-1] - off[0]) / 2.0
        traced = sorted(r[2] / max(r[2] + r[3], 1) for r in sides["on"])[len(on) // 2]
        deltas.append(delta)
        floors.append(floor)
        region = f"h{height}m"
        rows.append((Row("RCG-FLAT", "recede-lab", region, "gi_cost_ms", delta, floor, 0.0,
                         passed="report", reference_spp=0), ""))
        rows.append((Row("RCG-FLAT", "recede-lab", region, "traced_fraction", traced, 0.0, 0.0,
                         passed="report", reference_spp=0), ""))
    spread, ratio = flatness_stats(deltas, floors)
    judged = FLATNESS_BUDGET is not None
    rows.append((Row("RCG-FLAT", "recede-lab", "all", "cost_spread_ms", spread, max(floors), 0.0,
                     passed="report", reference_spp=0), ""))
    rows.append((Row("RCG-FLAT", "recede-lab", "all", "cost_flatness", ratio, max(floors), 0.0,
                     passed=("yes" if ratio <= FLATNESS_BUDGET else "no") if judged else "report",
                     threshold=f"<={FLATNESS_BUDGET}" if judged else "", reference_spp=0), ""))
    return rows


# ── Penumbra: A2, A4 (Phase 3) ───────────────────────────────────────────

PENUMBRA_RUNGS = ("penumbra", "penumbra-sun")
# At 1024 spp the finite solar disk's reference noise can leak into A4's
# fitted umbra band and raise the p95 floor above its frozen drift margin.
SUN_PENUMBRA_REFERENCE_SPP = 16384
DBG_DISABLE_DIRECT_SHADOWS = 0x8000000  # crates/renderer/src/shader_constants_data.rs
PENUMBRA_MIN_PX = 10  # RLT-SHD-001: the reference penumbra spans at least 10 px


@dataclass
class RungFile:
    """`rung.tsv`: the measurement geometry the rung fixes in its own code."""
    step_form: str = ""
    min_unshadowed: float = 0.0
    casters: list[str] = field(default_factory=list)
    # (name, receiver, start, end, strip half-width), world units
    profiles: list[tuple[str, str, np.ndarray, np.ndarray, float]] = field(default_factory=list)
    # (name, receiver, box min, box max), world units
    regions: list[tuple[str, str, np.ndarray, np.ndarray]] = field(default_factory=list)
    # (name, receiver, centre x, centre z, half x, half z, yaw, width), world units;
    # a moving caster's band is written at the exported frame's pose
    bands: list[tuple[str, str, float, float, float, float, float, float]] = field(default_factory=list)


def read_rung(scene_dir: Path) -> RungFile:
    rung = RungFile()
    for raw in (scene_dir / "rung.tsv").read_text().splitlines():
        if not raw or raw.startswith("#"):
            continue
        f = raw.split("\t")
        if f[0] == "step_form":
            rung.step_form = f[1]
        elif f[0] == "min_unshadowed":
            rung.min_unshadowed = float(f[1])
        elif f[0] == "caster":
            rung.casters.append(f[1])
        elif f[0] == "profile":
            v = np.array([float(x) for x in f[3:10]])
            rung.profiles.append((f[1], f[2], v[0:3], v[3:6], float(v[6])))
        elif f[0] == "region":
            v = np.array([float(x) for x in f[3:9]])
            rung.regions.append((f[1], f[2], v[0:3], v[3:6]))
        elif f[0] == "band":
            rung.bands.append((f[1], f[2], *(float(x) for x in f[3:9])))
    return rung


def without_casters(scene_dir: Path, casters: list[str]) -> Path:
    """The exported scene with the rung's casters removed (§2 visibility)."""
    out = scene_dir.parent / f"{scene_dir.name}-unoccluded"
    if out.exists():
        shutil.rmtree(out)
    shutil.copytree(scene_dir / "meshes", out / "meshes")
    # Normal-map and cut-out bitmaps (RLT-EXP-002) the remaining shapes
    # reference. Their names hash their pixels, so scene.xml's bytes (and
    # the reference cache key) already cover their content.
    if (scene_dir / "textures").is_dir():
        shutil.copytree(scene_dir / "textures", out / "textures")
    tree = ET.parse(scene_dir / "scene.xml")
    root = tree.getroot()
    for shape in list(root.findall("shape")):
        if shape.get("id") in casters:
            root.remove(shape)
    tree.write(out / "scene.xml", encoding="utf-8", xml_declaration=True)
    return out


def visibility(shadowed: np.ndarray, unshadowed: np.ndarray, receiver: np.ndarray,
               min_unshadowed: float) -> tuple[np.ndarray, int]:
    """Per-pixel shadowed / unshadowed luminance on `receiver`, NaN elsewhere.

    Pixels whose unshadowed luminance is below `min_unshadowed` of the
    receiver's brightest are excluded and counted (§2).
    """
    u, lit = luminance(unshadowed), luminance(shadowed)
    keep = receiver & (u > min_unshadowed * float(u[receiver].max()))
    v = np.full(u.shape, np.nan)
    v[keep] = lit[keep] / u[keep]
    return v, int((receiver & ~keep).sum())


def disk_segment(x: np.ndarray) -> np.ndarray:
    """Uncovered fraction of a unit disk whose centre lies `x` radii past a
    straight edge: the visibility behind an edge under a disk source."""
    x = np.clip(x, -1.0, 1.0)
    return (np.arccos(-x) + x * np.sqrt(1.0 - x * x)) / np.pi


STEP_FORMS = {"disk_segment": disk_segment}


def step_quantile(form, q: float) -> float:
    lo, hi = -1.0, 1.0
    for _ in range(60):
        mid = 0.5 * (lo + hi)
        lo, hi = (mid, hi) if form(np.array(mid)) < q else (lo, mid)
    return 0.5 * (lo + hi)


@dataclass
class StepFit:
    centre: float      # along the profile line, world units
    radius: float      # half the full penumbra, world units
    width_10_90: float  # world units


def fit_step(form, t: np.ndarray, v: np.ndarray) -> StepFit:
    """Least squares of lo + (hi - lo) * form((t - centre) / radius).

    lo and hi are solved linearly for each (centre, radius); the two are
    found by a grid search refined five times around the best cell.
    """
    def residual(centre: float, radius: float) -> float:
        f = form((t - centre) / radius)
        a = np.stack([1.0 - f, f], axis=1)
        coef, *_ = np.linalg.lstsq(a, v, rcond=None)
        r = a @ coef - v
        return float(r @ r)

    span = float(t.max() - t.min())
    centres = np.linspace(t.min(), t.max(), 61)
    radii = np.geomspace(span / 500.0, span, 61)
    step, factor = span / 60.0, (500.0 ** (1.0 / 60.0))
    best = (math.inf, 0.0, 0.0)
    for _ in range(6):
        for c in centres:
            for r in radii:
                e = residual(c, r)
                if e < best[0]:
                    best = (e, c, r)
        _, c0, r0 = best
        centres = np.linspace(c0 - step, c0 + step, 21)
        radii = r0 * np.geomspace(1.0 / factor, factor, 21)
        step, factor = step / 10.0, factor ** 0.1
    _, centre, radius = best
    width = (step_quantile(form, 0.9) - step_quantile(form, 0.1)) * radius
    return StepFit(float(centre), float(radius), float(width))


def line_coords(positions: np.ndarray, receiver: np.ndarray, start: np.ndarray, end: np.ndarray,
                half_width: float) -> tuple[np.ndarray, np.ndarray, float]:
    """Distance along the line per pixel, and the strip's pixels."""
    axis = end - start
    length = float(np.linalg.norm(axis))
    u = axis / length
    rel = positions - start
    t = rel @ u
    perp = np.linalg.norm(rel - t[..., None] * u, axis=-1)
    return t, receiver & (t >= 0.0) & (t <= length) & (perp <= half_width), length


def pixel_footprint(positions: np.ndarray, receiver: np.ndarray) -> float:
    """Median world distance between horizontally adjacent receiver pixels."""
    both = receiver[:, 1:] & receiver[:, :-1]
    return float(np.median(np.linalg.norm(positions[:, 1:] - positions[:, :-1], axis=-1)[both]))


def strip_profile(v: np.ndarray, t: np.ndarray, strip: np.ndarray, length: float,
                  bin_width: float) -> tuple[np.ndarray, np.ndarray]:
    """The strip averaged across its width: one mean per one-pixel bin."""
    tv, vv = t[strip], v[strip]
    keep = ~np.isnan(vv)
    tv, vv = tv[keep], vv[keep]
    bins = max(int(length / bin_width), 8)
    index = np.clip((tv / length * bins).astype(int), 0, bins - 1)
    centres, means = [], []
    for i in range(bins):
        sel = index == i
        if sel.sum() >= 3:
            centres.append((i + 0.5) * length / bins)
            means.append(float(vv[sel].mean()))
    return np.array(centres), np.array(means)


def visibility_error(va: np.ndarray, vb: np.ndarray, mask: np.ndarray) -> tuple[float, float]:
    """Mean and 95th percentile of |Va - Vb| over `mask` (§2)."""
    both = mask & ~np.isnan(va) & ~np.isnan(vb)
    if not both.any():
        raise SystemExit("m-light: a visibility region has no unexcluded pixel")
    err = np.abs(va[both] - vb[both])
    return float(err.mean()), float(np.percentile(err, 95))


def gate_penumbra(engine: Engine, store: ReferenceStore, ctx: Context, spp: int, gates: set[str]):
    """A2 penumbra width (RLT-SHD-001) and A4 filter bands and thin rod
    (RLT-SHD-004) on the local-light and sun penumbra rungs.

    Visibility (§2) on both sides: the engine's filtered direct view over the
    same view with DBG_DISABLE_DIRECT_SHADOWS, tier 3, unjittered; the
    reference direct recipe over the same recipe with the rung's casters
    removed. Report-only until the owner sets the L1 budgets (Q6).
    """
    del ctx
    rows = []
    sun_spp = max(spp, SUN_PENUMBRA_REFERENCE_SPP)
    extra = ["--upscaler", "taa", "--rt-test-no-jitter"]
    for rung_name in PENUMBRA_RUNGS:
        reference_spp = sun_spp if rung_name == "penumbra-sun" else spp
        export = engine.out / "scenes" / rung_name
        shadowed = [load_exr(engine.capture(f"{rung_name}_fdirect", rung_name, "filtered_direct", s, 3,
                                            extra=extra, export_dir=export if s == 0 else None))
                    for s in (0, 1)]
        unshadowed = [load_exr(engine.capture(f"{rung_name}_fdirect_unshadowed", rung_name, "filtered_direct",
                                              s, 3, extra=extra, debug_flags=hex(DBG_DISABLE_DIRECT_SHADOWS)))
                      for s in (0, 1)]
        rung = read_rung(export)
        form = STEP_FORMS[rung.step_form]
        opened = without_casters(export, rung.casters)
        ref = [store.render(export, "direct", reference_spp, s) for s in (0, 1)]
        ref_open = [store.render(opened, "direct", reference_spp, s) for s in (0, 1)]
        key = ref[0][1]
        masks, positions = pixel_hits(export)

        cache: dict[str, tuple] = {}

        def side_visibility(receiver_name: str):
            """(receiver mask, engine V per seed, reference V per seed, excluded)."""
            if receiver_name not in cache:
                receiver = erode(masks[receiver_name], ERODE_PX)
                ve = [visibility(shadowed[s], unshadowed[s], receiver, rung.min_unshadowed) for s in (0, 1)]
                vr = [visibility(ref[s][0], ref_open[s][0], receiver, rung.min_unshadowed) for s in (0, 1)]
                cache[receiver_name] = (receiver, [v for v, _ in ve], [v for v, _ in vr], ve[0][1] + vr[0][1])
            return cache[receiver_name]

        regions: dict[str, tuple[np.ndarray, str]] = {}
        for name, receiver_name, start, end, half_width in rung.profiles:
            receiver, ve, vr, excluded = side_visibility(receiver_name)
            t, strip, length = line_coords(positions, receiver, start, end, half_width)
            footprint = pixel_footprint(positions, receiver)

            def fit(v):
                return fit_step(form, *strip_profile(v, t, strip, length, footprint))

            fe, fr = [fit(v) for v in ve], [fit(v) for v in vr]
            width_px = fr[0].width_10_90 / footprint
            if width_px < PENUMBRA_MIN_PX:
                raise SystemExit(f"m-light: {rung_name}/{name}: the reference penumbra spans {width_px:.1f} px, "
                                 f"under RLT-SHD-001's {PENUMBRA_MIN_PX}; move the camera closer")

            def width_error(e: StepFit, r: StepFit) -> float:
                return abs(e.width_10_90 - r.width_10_90) / r.width_10_90

            value = width_error(fe[0], fr[0])
            if "A2" in gates:
                rows.append((Row("A2", rung_name, name, "penumbra_width_error", value,
                                 abs(width_error(fe[1], fr[0]) - value), abs(width_error(fe[0], fr[1]) - value),
                                 passed="report"), key))
                rows.append((Row("A2", rung_name, name, "reference_width_px", width_px, 0.0,
                                 abs(fr[1].width_10_90 - fr[0].width_10_90) / footprint, passed="report"), key))
                rows.append((Row("A2", rung_name, name, "excluded_pixels", float(excluded), 0.0, 0.0,
                                 passed="report"), key))
            # A4's bands just outside the reference penumbra, one 10-90 %
            # width deep, on the umbra and the lit side.
            r0 = fr[0]
            for band, lo, hi in (("umbra_band", r0.centre - r0.radius - r0.width_10_90, r0.centre - r0.radius),
                                 ("lit_band", r0.centre + r0.radius, r0.centre + r0.radius + r0.width_10_90)):
                band_mask = strip & (t >= lo) & (t <= hi)
                previous = regions.get(band, (np.zeros_like(receiver), receiver_name))[0]
                regions[band] = (previous | band_mask, receiver_name)
        for name, receiver_name, lo, hi in rung.regions:
            receiver = side_visibility(receiver_name)[0]
            inside = np.all((positions >= lo) & (positions <= hi), axis=-1)
            regions[name] = (receiver & inside, receiver_name)
        if "A4" not in gates:
            continue
        for region, (mask, receiver_name) in regions.items():
            _, ve, vr, _ = side_visibility(receiver_name)
            mean, p95 = visibility_error(ve[0], vr[0], mask)
            mean_e, p95_e = visibility_error(ve[1], vr[0], mask)
            mean_r, p95_r = visibility_error(ve[0], vr[1], mask)
            rows.append((Row("A4", rung_name, region, "visibility_error_mean", mean,
                             abs(mean_e - mean), abs(mean_r - mean), passed="report"), key))
            rows.append((Row("A4", rung_name, region, "visibility_error_p95", p95,
                             abs(p95_e - p95), abs(p95_r - p95), passed="report"), key))
    for row, _ in rows:
        if row.case == "penumbra-sun":
            row.reference_spp = sun_spp
    return rows


# ── Contact: A5 (Phase 3) ────────────────────────────────────────────────

CONTACT_TIERS = (1, 2, 3)  # RLT-CON-001: tiers 1-3, an exception to Q7's tier-3 rule
# The plank band is the one contact band full of partially shadowed pixels, and
# a single 1024-spp Mitsuba render carries a per-pixel visibility noise of about
# 3.2e-3 mean / 1.7e-2 p95 there (two seeds differ by 4.5e-3 mean), as much as
# the engine's whole converged error and over the plank's budget (2e-3 / 1e-2).
# Sixteen times the samples quarters it (PRD section 12, revision 28).
CONTACT_REFERENCE_SPP_FACTOR = 16


def band_mask(positions: np.ndarray, receiver: np.ndarray, band) -> np.ndarray:
    """Receiver pixels within `width` of the caster's footprint, outside it.

    The footprint is a rectangle turned by `yaw` about +Y (glam's
    `Quat::from_rotation_y`: world x = cos·x + sin·z, world z = −sin·x + cos·z).
    """
    _, _, cx, cz, hx, hz, yaw, width = band
    dx, dz = positions[..., 0] - cx, positions[..., 2] - cz
    c, s = math.cos(yaw), math.sin(yaw)
    lx, lz = c * dx - s * dz, s * dx + c * dz
    outer = (np.abs(lx) <= hx + width) & (np.abs(lz) <= hz + width)
    inner = (np.abs(lx) <= hx) & (np.abs(lz) <= hz)
    return receiver & outer & ~inner


# RLT-SHD-012's control: the same rung with the floor's normal map off. The
# plank is the band the normal map disturbs, so only its row is reported, at
# the last tier, beside the normal-mapped plank's.
CONTROL_RUNG = "contact-flat"
CONTROL_BAND = "con_plank_band"


# ── Sky IBL (Phase 4: A6, RLT-IBL-001…004, RLT-REF-012, -019, -021) ──────

SKY_ENV_FACES = 6
SKY_ENV_FACE_PX = 512          # four times SKY_CUBE_FACE_SIZE (RLT-REF-012)
SKY_ENV_LATLONG = (2048, 1024)  # the same angular resolution at the equator
SKY_ENV_FILE = "sky_env.exr"
SKY_PROBE_ROUGHNESS = (0.02, 0.26, 0.50, 0.74, 0.98)


def pixel_directions(scene_dir: Path) -> np.ndarray:
    """The world-space ray direction through each pixel centre of the
    exported camera, shape (height, width, 3) — the rays the engine shades."""
    import drjit as dr

    scene = mi.load_file(str(scene_dir / "scene.xml"), optimize=False)
    sensor = scene.sensors()[0]
    width, height = sensor.film().size()
    index = dr.arange(mi.UInt32, width * height)
    uv = mi.Point2f((mi.Float(index % width) + 0.5) / width,
                    (mi.Float(index // width) + 0.5) / height)
    rays, _ = sensor.sample_ray(0.0, 0.0, uv, mi.Point2f(0.5, 0.5))
    return np.array(rays.d).T.reshape(height, width, 3)


@dataclass
class PinholeView:
    """A pinhole camera recovered from its per-pixel ray directions:
    pixel = (a·d / f·d, b·d / f·d) + offset, exact for a perspective view."""
    forward: np.ndarray
    a: np.ndarray
    b: np.ndarray
    offset: np.ndarray
    image: np.ndarray

    @staticmethod
    def fit(directions: np.ndarray, image: np.ndarray) -> "PinholeView":
        h, w, _ = directions.shape
        # The optical axis: the mean of the four corner rays, exact for the
        # symmetric frustum both renderers use (an even-sized image has no
        # pixel on the axis itself).
        f = directions[[0, 0, -1, -1], [0, -1, 0, -1]].sum(axis=0)
        f = f / np.linalg.norm(f)
        d = directions.reshape(-1, 3)
        proj = d / (d @ f)[:, None]            # onto the plane f·d = 1
        ys, xs = np.mgrid[0:h, 0:w]
        A = np.c_[proj, np.ones(len(proj))]
        cx, *_ = np.linalg.lstsq(A, xs.ravel() + 0.5, rcond=None)
        cy, *_ = np.linalg.lstsq(A, ys.ravel() + 0.5, rcond=None)
        resid = max(np.abs(A @ cx - xs.ravel() - 0.5).max(), np.abs(A @ cy - ys.ravel() - 0.5).max())
        if resid > 1e-2:
            raise SystemExit(f"m-light: sky-env view is not a pinhole camera (residual {resid:.3g} px)")
        return PinholeView(f, cx[:3], cy[:3], np.array([cx[3], cy[3]]), image)

    def sample(self, d: np.ndarray) -> np.ndarray:
        """Bilinear radiance along unit directions `d` (N, 3) in front of the view."""
        h, w, _ = self.image.shape
        z = d @ self.forward
        x = (d @ self.a) / z + self.offset[0] - 0.5
        y = (d @ self.b) / z + self.offset[1] - 0.5
        x0 = np.clip(np.floor(x).astype(int), 0, w - 2)
        y0 = np.clip(np.floor(y).astype(int), 0, h - 2)
        # Unclamped weights: a texel within half a pixel of the view's edge
        # lies outside the pixel-centre grid and is linearly extrapolated.
        tx = (x - x0)[:, None]
        ty = (y - y0)[:, None]
        im = self.image
        top = im[y0, x0] * (1 - tx) + im[y0, x0 + 1] * tx
        bottom = im[y0 + 1, x0] * (1 - tx) + im[y0 + 1, x0 + 1] * tx
        return top * (1 - ty) + bottom * ty


def latlong_directions(width: int, height: int) -> np.ndarray:
    """Texel-centre directions of a Mitsuba 3.9.1 `envmap`: u = atan2(x, -z) / 2π,
    v = acos(y) / π, +Y up (measured on 3.9.1; the engine is +Y up too)."""
    u = (np.arange(width) + 0.5) / width
    v = (np.arange(height) + 0.5) / height
    phi, theta = np.meshgrid(u * 2.0 * np.pi, v * np.pi)
    sin_t = np.sin(theta)
    return np.stack([sin_t * np.sin(phi), np.cos(theta), -sin_t * np.cos(phi)], axis=-1)


def assemble_sky_env(views: list[PinholeView], width: int, height: int) -> np.ndarray:
    """Resample the six 90° captures into a lat-long map: each texel from the
    view whose axis it is nearest, which for six axis-aligned 90° views is
    the view that contains it."""
    dirs = latlong_directions(width, height).reshape(-1, 3)
    axes = np.stack([v.forward for v in views])
    pick = np.argmax(dirs @ axes.T, axis=1)
    out = np.zeros((dirs.shape[0], 3))
    for i, view in enumerate(views):
        sel = pick == i
        out[sel] = view.sample(dirs[sel])
    return out.reshape(height, width, 3)


def capture_sky_env(engine: Engine, ctx: Context) -> Path:
    """RLT-REF-012 / RLT-EXP-003: the analytic sky without its disc, at four
    times the engine cube's face resolution, as the lat-long `sky_env.exr`."""
    path = engine.out / "scenes" / "sky-env" / SKY_ENV_FILE
    views = []
    for face in range(SKY_ENV_FACES):
        export = engine.out / "scenes" / f"sky-env-f{face}"
        exr = engine.capture(f"sky_env_f{face}", "sky-env", "final", 0, ctx.tier, reference_view=face,
                             export_dir=export,
                             extra=["--upscaler", "taa", "--rt-test-no-jitter",
                                    "--window-size", f"{SKY_ENV_FACE_PX}x{SKY_ENV_FACE_PX}"])
        views.append(PinholeView.fit(pixel_directions(export), load_exr(exr)))
    env = assemble_sky_env(views, *SKY_ENV_LATLONG)
    path.parent.mkdir(parents=True, exist_ok=True)
    mi.Bitmap(env.astype(np.float32)).write(str(path))
    return path


def sky_probe_case(view: int) -> str:
    if view < 5:
        return f"dielectric-r{SKY_PROBE_ROUGHNESS[view]:.2f}"
    if view < 10:
        return f"conductor-r{SKY_PROBE_ROUGHNESS[view - 5]:.2f}"
    return "lambertian"


def sky_energy_rows(case: str, ea, eb, ra, rb, mask) -> list[Row]:
    """§2 relative error distribution over the probe, normalised by the
    region's mean reference value of the same component, and the energy
    ratio engine ÷ reference (a ratio below 1 is missing sky light)."""
    def rel(e, r):
        norm = region_mean(r, mask)
        err = np.abs(luminance(e)[mask] - luminance(r)[mask]) / norm
        return float(err.mean()), float(np.percentile(err, 95))

    rows = []
    value = ratio(ea, ra, mask)
    rows.append(Row("A6", case, "sky_probe", "energy_ratio", value,
                    abs(ratio(eb, ra, mask) - value), abs(ratio(ea, rb, mask) - value), passed="report"))
    (m_a, p_a), (m_b, p_b), (m_r, p_r) = rel(ea, ra), rel(eb, ra), rel(ea, rb)
    rows.append(Row("A6", case, "sky_probe", "rel_error_mean", m_a, abs(m_b - m_a), abs(m_r - m_a),
                    passed="report"))
    rows.append(Row("A6", case, "sky_probe", "rel_error_p95", p_a, abs(p_b - p_a), abs(p_r - p_a),
                    passed="report"))
    return rows


def gate_sky_ibl(engine: Engine, store: ReferenceStore, ctx: Context, spp: int):
    """A6 (RLT-IBL-001…004 baseline): sky specular on the specular-only
    probes and sky diffuse on the Lambertian one, probe alone (RLT-REF-021),
    directional dropped (RLT-REF-019), against the analytic environment.
    Report-only until the owner writes the L3 budgets (Q6)."""
    env = capture_sky_env(engine, ctx)
    rows = []
    native = ["--upscaler", "taa", "--rt-test-no-jitter"]
    for view in range(11):
        case = sky_probe_case(view)
        export = engine.out / "scenes" / f"sky-probe-v{view}"
        # Specular probes: `direct_only` holds the environment specular alone
        # (no directional, no emitter). Lambertian: `indirect_only` x albedo 1.
        mode = "indirect_only" if view == 10 else "direct_only"
        ea = engine.capture(f"sky_probe_v{view}", "sky-probe", mode, 0, ctx.tier, reference_view=view,
                            export_dir=export, extra=native)
        eb = engine.capture(f"sky_probe_v{view}", "sky-probe", mode, 1, ctx.tier, reference_view=view,
                            extra=native)
        shutil.copyfile(env, export / SKY_ENV_FILE)
        ra, key = store.render(export, "direct", spp, 0)
        rb, _ = store.render(export, "direct", spp, 1)
        mask = region_masks(export)["sky_probe"]
        rows += [(row, key) for row in sky_energy_rows(case, load_exr(ea), load_exr(eb), ra, rb, mask)]
    return rows


def gate_far_sun(engine: Engine, ctx: Context):
    """RLT-CON-004: traced, screen-space, and off-screen caster controls."""
    def capture(name: str, pose: int, mode: str, disable: bool = False) -> np.ndarray:
        return load_exr(engine.capture(
            f"far_sun_{name}", "far-sun", mode, 0, ctx.tier, reference_view=pose,
            extra=["--upscaler", "taa", "--rt-test-no-jitter"],
            debug_flags=hex(DBG_DISABLE_DIRECT_SHADOWS) if disable else "0",
        ))

    near = capture("near", 0, "filtered_direct")
    near_light = capture("near_unshadowed", 0, "filtered_direct", disable=True)
    far = capture("far", 1, "filtered_direct")
    far_raw = capture("far_raw", 1, "direct_only")
    far_disabled = capture("far_disabled", 1, "filtered_direct", disable=True)
    offscreen = capture("offscreen", 2, "filtered_direct")
    offscreen_raw = capture("offscreen_raw", 2, "direct_only")

    # Ignore differences of one half-float step near unit radiance.
    epsilon = F16_REPRESENTATION
    far_loss = far_raw - far
    darkened = far_loss.max(axis=2) > epsilon
    near_count = np.count_nonzero((near_light - near).max(axis=2) > epsilon)
    blue_count = np.count_nonzero(darkened & (far[..., 2] > 0.01))
    checks = [
        ("near", "darkened_pixels", float(near_count), ">0", near_count > 0),
        ("far", "darkened_pixels", float(darkened.sum()), ">0", darkened.any()),
        ("far", "max_channel_gain", float((-far_loss).max()), "<=2^-10",
         far_loss.min() >= -epsilon),
        ("far", "darkened_pixels_with_blue_fill", float(blue_count), ">0", blue_count > 0),
        ("far_disabled", "max_absolute_change", float(np.abs(far_raw - far_disabled).max()),
         "<=2^-10", np.all(np.abs(far_raw - far_disabled) <= epsilon)),
        ("offscreen", "max_absolute_change", float(np.abs(offscreen_raw - offscreen).max()),
         "<=2^-10", np.all(np.abs(offscreen_raw - offscreen) <= epsilon)),
    ]
    return [(Row("CON-004", case, "frame", metric, value, 0.0, 0.0,
                 passed="yes" if passed else "no", threshold=limit,
                 reference_spp=0), "")
            for case, metric, value, limit, passed in checks]


def gate_contact(engine: Engine, store: ReferenceStore, ctx: Context, spp: int):
    """A5 contact bands (RLT-CON-001, -002, -006, -007, -008).

    Visibility (§2) per caster's contact band, the engine's filtered direct
    view (shadow filter on, RLT-CON-002) and `direct_only` (off) at tiers
    1-3, unjittered, against the reference's direct recipe over the same with
    the casters removed. The threshold table judges the rows it names; the
    rest are reported. The flat-floor control (RLT-SHD-012, #6721) follows
    the normal-mapped rows: the plank on the same rung without the map.
    """
    del ctx
    spp *= CONTACT_REFERENCE_SPP_FACTOR
    rows = contact_band_rows(engine, store, "contact", spp, CONTACT_TIERS, None, "")
    rows += contact_band_rows(engine, store, CONTROL_RUNG, spp, CONTACT_TIERS[-1:], CONTROL_BAND, "-flatfloor")
    rows += contact_mid_animation(engine, store, spp)
    for row, _ in rows:
        row.reference_spp = spp
    return rows


def contact_band_rows(engine: Engine, store: ReferenceStore, rung_name: str, spp: int, tiers,
                      only_band: str | None, case_suffix: str):
    """A5 rows for one contact rung: every band (or `only_band`) at `tiers`.

    `case_suffix` goes on the case name, so the control's rows never share a
    threshold key with the normal-mapped rung's."""
    rows = []
    export = engine.out / "scenes" / rung_name
    extra = ["--upscaler", "taa", "--rt-test-no-jitter"]
    exported = False
    reference = None
    for view_mode in ("filtered_direct", "direct_only"):
        for tier in tiers:
            name = f"{rung_name.replace('-', '_')}_{view_mode}_t{tier}"
            shadowed = [load_exr(engine.capture(name, rung_name, view_mode, s, tier, extra=extra,
                                                export_dir=export if not exported and s == 0 else None))
                        for s in (0, 1)]
            exported = True
            unshadowed = [load_exr(engine.capture(f"{name}_unshadowed", rung_name, view_mode, s, tier, extra=extra,
                                                  debug_flags=hex(DBG_DISABLE_DIRECT_SHADOWS)))
                          for s in (0, 1)]
            if reference is None:
                rung = read_rung(export)
                opened = without_casters(export, rung.casters)
                ref = [store.render(export, "direct", spp, s) for s in (0, 1)]
                ref_open = [store.render(opened, "direct", spp, s) for s in (0, 1)]
                masks, positions = pixel_hits(export)
                receivers = {band[1]: erode(masks[band[1]], ERODE_PX) for band in rung.bands}
                vr = {r: [visibility(ref[s][0], ref_open[s][0], m, rung.min_unshadowed)[0] for s in (0, 1)]
                      for r, m in receivers.items()}
                reference = (rung, ref[0][1], positions, receivers, vr)
            rung, key, positions, receivers, vr = reference
            for band in rung.bands:
                if only_band is not None and band[0] != only_band:
                    continue
                receiver_name = band[1]
                receiver = receivers[receiver_name]
                mask = band_mask(positions, receiver, band)
                ve = [visibility(shadowed[s], unshadowed[s], receiver, rung.min_unshadowed)[0] for s in (0, 1)]
                ref_v = vr[receiver_name]
                mean, p95 = visibility_error(ve[0], ref_v[0], mask)
                mean_e, p95_e = visibility_error(ve[1], ref_v[0], mask)
                mean_r, p95_r = visibility_error(ve[0], ref_v[1], mask)
                case = f"{view_mode}-tier{tier}{case_suffix}"
                rows.append((Row("A5", case, band[0], "visibility_error_mean", mean,
                                 abs(mean_e - mean), abs(mean_r - mean), passed="report"), key))
                rows.append((Row("A5", case, band[0], "visibility_error_p95", p95,
                                 abs(p95_e - p95), abs(p95_r - p95), passed="report"), key))
                if tier == tiers[-1] and view_mode == "filtered_direct":
                    rows.append((Row("A5", case, band[0], "band_pixels", float(mask.sum()), 0.0, 0.0,
                                     passed="report"), key))
    return rows


# RLT-CON-003: the skinned proxy mid-animation, at this bench frame of a
# renderer-stepped run. The animation steps 1/60 s before frame 0 is drawn,
# so drawn frame F is clip time (F + 1) / 60 s: frame 45 is t = 46/60 s of
# the 2 s sway, whose first key ramps yaw at 0.6 rad/s (`reference.rs`),
# so yaw 0.46 rad (V9, #6605, measured 2026-11-04).
PROXY_FRAME = 45
PROXY_YAW_RATE = 0.6
PROXY_YAW_TOLERANCE = 1e-4


def proxy_yaw(scene_dir: Path) -> float:
    """The exported proxy column's yaw about +Y, from its posed PLY: the
    top face's corners around their centre, less the 45° of a square."""
    root = ET.parse(scene_dir / "scene.xml").getroot()
    shape = next(s for s in root.iter("shape") if s.get("id") == "con_proxy")
    filename = next(e.get("value", "") for e in shape.iter("string") if e.get("name") == "filename")
    data = (scene_dir / filename).read_bytes()
    body = data[data.index(b"end_header\n") + len(b"end_header\n"):]
    vertices = np.frombuffer(body, dtype="<f4", count=24 * 6).reshape(24, 6)[:, :3]
    top = vertices[np.abs(vertices[:, 1] - vertices[:, 1].max()) < 1e-3]
    corners = np.unique(np.round(top[:, [0, 2]], 4), axis=0)
    centred = corners - corners.mean(axis=0)
    angles = np.arctan2(-centred[:, 1], centred[:, 0]) % (math.pi / 2)
    return float(angles.min() - math.pi / 4)


def contact_mid_animation(engine: Engine, store: ReferenceStore, spp: int):
    """A5's skinned-proxy row (RLT-CON-003): the proxy's band at one frame
    of its animation, against the reference rendered at the pose exported
    for that frame (RLT-EXP-001), and V9 (#6605): two runs must export
    byte-identical posed meshes at the same frame, at the clip time that
    frame index names."""
    rows = []
    rung_name, tier = "contact", CONTACT_TIERS[-1]
    exports = [engine.out / "scenes" / f"contact-proxy-f{PROXY_FRAME}-run{s}" for s in (0, 1)]
    # renderer-stepped needs an explicit path; `static` holds the rung's
    # camera while the animation clock steps.
    base = ["--rt-test-no-jitter", "--upscaler", "taa", "--bench-camera", "static"]
    shadowed, unshadowed = [], []
    for s in (0, 1):
        export = ["--export-mitsuba", str(exports[s]), "--export-mitsuba-frame", str(PROXY_FRAME)]
        seq = engine.sequence("contact_proxy", rung_name, tier, [PROXY_FRAME], PROXY_FRAME + 8,
                              base + export, view_mode="filtered_direct", seed=s)
        shadowed.append(load_exr(seq[PROXY_FRAME][0]))
        seq = engine.sequence("contact_proxy_unshadowed", rung_name, tier, [PROXY_FRAME], PROXY_FRAME + 8,
                              base, view_mode="filtered_direct", seed=s,
                              debug_flags=hex(DBG_DISABLE_DIRECT_SHADOWS))
        unshadowed.append(load_exr(seq[PROXY_FRAME][0]))
    # V9: the drawn pose at a frame index is a function of the index alone.
    plys = [sorted(p.read_bytes() for p in (e / "meshes").glob("*.ply")) for e in exports]
    identical = float(plys[0] == plys[1])
    rows.append((Row("V9", f"contact-proxy-frame{PROXY_FRAME}", "posed_meshes", "byte_identical_runs",
                     identical, 0.0, 0.0, passed="yes" if identical == 1.0 else "no"), ""))
    # A pose that never advanced would also be identical: the yaw must be
    # the clip's at frame index PROXY_FRAME.
    yaw_error = abs(proxy_yaw(exports[0]) - PROXY_YAW_RATE * (PROXY_FRAME + 1) / 60.0)
    rows.append((Row("V9", f"contact-proxy-frame{PROXY_FRAME}", "con_proxy", "yaw_error_rad",
                     yaw_error, 0.0, 0.0, passed="yes" if yaw_error <= PROXY_YAW_TOLERANCE else "no",
                     threshold=f"<={PROXY_YAW_TOLERANCE}"), ""))
    rung = read_rung(exports[0])
    opened = without_casters(exports[0], rung.casters)
    ref = [store.render(exports[0], "direct", spp, s) for s in (0, 1)]
    ref_open = [store.render(opened, "direct", spp, s) for s in (0, 1)]
    masks, positions = pixel_hits(exports[0])
    band = next(b for b in rung.bands if b[0] == "con_proxy_band")
    receiver = erode(masks[band[1]], ERODE_PX)
    mask = band_mask(positions, receiver, band)
    ve = [visibility(shadowed[s], unshadowed[s], receiver, rung.min_unshadowed)[0] for s in (0, 1)]
    vr = [visibility(ref[s][0], ref_open[s][0], receiver, rung.min_unshadowed)[0] for s in (0, 1)]
    mean, p95 = visibility_error(ve[0], vr[0], mask)
    mean_e, p95_e = visibility_error(ve[1], vr[0], mask)
    mean_r, p95_r = visibility_error(ve[0], vr[1], mask)
    case = f"filtered_direct-tier{tier}-frame{PROXY_FRAME}"
    rows.append((Row("A5", case, band[0], "visibility_error_mean", mean, abs(mean_e - mean), abs(mean_r - mean),
                     passed="report"), ref[0][1]))
    rows.append((Row("A5", case, band[0], "visibility_error_p95", p95, abs(p95_e - p95), abs(p95_r - p95),
                     passed="report"), ref[0][1]))
    return rows


# ── RLT-SHD-005: the reservoir weight cap's bias ────────────────────────


def gate_reservoir_clamp(engine: Engine, store: ReferenceStore, ctx: Context, spp: int):
    """RLT-SHD-005: the luminance-integral delta of `direct_only`, cap on
    against off (`--rt-test-no-reservoir-clamp`), on the dim-plus-hero box at
    tier 3, over all geometry. Published, not gated."""
    del store, spp
    export = engine.out / "scenes" / "restir-clamp"
    on = [load_exr(engine.capture("restir_clamp_on", "restir-clamp", "direct_only", s, ctx.tier,
                                  export_dir=export if s == 0 else None)) for s in (0, 1)]
    off = [load_exr(engine.capture("restir_clamp_off", "restir-clamp", "direct_only", s, ctx.tier,
                                   extra=["--rt-test-no-reservoir-clamp"])) for s in (0, 1)]
    masks = region_masks(export, erode_px=0)
    geometry = np.logical_or.reduce([m for name, m in masks.items() if "emitter" not in name])

    def integral(image):
        return float(luminance(image)[geometry].sum())

    value = integral(on[0]) / integral(off[0]) - 1.0
    floor_on = abs(integral(on[1]) / integral(off[0]) - 1.0 - value)
    floor_off = abs(integral(on[0]) / integral(off[1]) - 1.0 - value)
    row = Row("SHD-005", "restir-clamp", "all_geometry", "luminance_integral_delta", value,
              floor_on, floor_off, passed="report")
    return [(row, "")]


# ── Temporal: A3, A14 (Phase 3) ──────────────────────────────────────────

# The motion spans TEMPORAL_MOTION frames (`--bench-camera-length`), then
# holds; samples every TEMPORAL_STRIDE frames from TEMPORAL_FIRST (past the
# cold start) during it, and at 1, 4 and 16 frames after the stop
# (RLT-SHD-002, -010). Each sample's parked reference freezes the same path
# at that frame and converges for PARKED_SETTLE frames.
TEMPORAL_MOTION = 120
TEMPORAL_STRIDE = 16
TEMPORAL_FIRST = 32
TEMPORAL_POST_STOP = (1, 4, 16)
PARKED_SETTLE = 256


def temporal_error(moving: np.ndarray, parked: np.ndarray, mask: np.ndarray) -> float:
    """Relative RMSE over `mask`, normalised by the parked mean (§2)."""
    m, p = luminance(moving)[mask], luminance(parked)[mask]
    return float(np.sqrt(np.mean((m - p) ** 2)) / max(float(p.mean()), 1e-9))


def gate_temporal(engine: Engine, store: ReferenceStore, ctx: Context, spp: int, gates: set[str],
                  seed: int = 0, debug_flags: str = "0"):
    """A14 moving blocker (RLT-SHD-010) and A3 camera pan (RLT-SHD-002):
    compare each moving capture with the engine parked at the same pose.
    A3 uses direct light and age-matched held controls; A14 retains final."""
    del store, spp
    rows = []
    stop = TEMPORAL_MOTION - 1
    motion = list(range(TEMPORAL_FIRST, TEMPORAL_MOTION, TEMPORAL_STRIDE))
    post = [stop + k for k in TEMPORAL_POST_STOP]
    bench_frames = post[-1] + 4
    # The temporal path is what these gates measure, so the captures tap
    # TAA's resolved output (`--linear-capture-tap post-taa`), not the
    # unreconstructed image the reference gates compare.
    clock = ["--bench-camera-length", str(TEMPORAL_MOTION), "--upscaler", "taa",
             "--linear-capture-tap", "post-taa"]
    cases = []
    if "A14" in gates:
        # The blocker moves on the bench clock; the camera holds still (a
        # parked path would freeze the blocker too, it shares the clock).
        cases.append(("A14", "penumbra-moving", clock + ["--bench-camera", "static"]))
    if "A3" in gates:
        cases.append(("A3", "penumbra", ["--bench-camera-length", str(TEMPORAL_MOTION),
                                          "--upscaler", "taa", "--bench-camera", "pan"]))
    if "SHD-006" in gates:
        # A3 now measures direct light, so retain a separate final-image
        # baseline for the TAA gamma comparison at the shipped 1.5.
        cases.append(("SHD-006-BASE", "penumbra", clock + ["--bench-camera", "pan"]))
        cases.append(("SHD-006", "penumbra", clock + ["--bench-camera", "pan", "--rt-test-taa-gamma", "1.0"]))
    for gate, rung, extra in cases:
        view = "direct_only" if gate == "A3" else "final"
        export = engine.out / "scenes" / f"{rung}-temporal"
        # The receiver in view at the rung's own (start) pose; the pan's
        # later poses see a rotated view of the same floor, so its mask is
        # taken from the parked capture's non-black floor pixels instead.
        engine.capture(f"{gate}_export", rung, "final", seed, ctx.tier,
                       extra=["--upscaler", "taa"], export_dir=export, debug_flags=debug_flags)
        masks = region_masks(export)
        floor = masks["pen_floor"]
        moving = engine.sequence(f"{gate}_{rung}_moving", rung, ctx.tier, motion + post, bench_frames, extra,
                                 view_mode=view, seed=seed, debug_flags=debug_flags)
        parked_at = {f: f for f in motion} | {f: stop for f in post}
        parked = {}
        for frame in sorted(set(parked_at.values())):
            parked[frame] = load_exr(engine.capture(
                f"{gate}_{rung}_parked_f{frame}", rung, view, seed, ctx.tier,
                extra=extra + ["--bench-camera-freeze-at", str(frame)],
                bench_frames=frame + PARKED_SETTLE, bench_mode="renderer-stepped", debug_flags=debug_flags))
        # The floor: the same single-frame captures with the path held at
        # the stop pose from frame 0 (`--bench-camera-hold-at`: no motion
        # history; `--bench-camera-freeze-at` still travels the path up to its
        # frame, so as a control it is the moving run itself), against the
        # same converged reference — what per-frame sampling noise alone
        # contributes.
        held_post = list(TEMPORAL_POST_STOP) if gate == "A3" else post
        still = engine.sequence(f"{gate}_{rung}_still", rung, ctx.tier, held_post, max(held_post) + 4,
                                extra + ["--bench-camera-hold-at", str(stop)], view_mode=view, seed=seed,
                                debug_flags=debug_flags)

        def mask_for(reference):
            return floor if gate == "A14" else erode(luminance(reference) > 1e-4, ERODE_PX)

        post_controls = {f: temporal_error(load_exr(still[f - stop][0]), parked[stop], mask_for(parked[stop]))
                         for f in post} if gate == "A3" else {}
        floor_e = (float(np.mean([temporal_error(load_exr(still[f][0]), parked[stop], mask_for(parked[stop]))
                                  for f in post])) if gate != "A3" else 0.0)
        # A sample during the motion also carries the run's cold start, since
        # the temporal accumulators are still filling at frame 32. Its floor
        # is therefore per frame: the same single frame of a run held at that
        # frame's pose from frame 0, against the same converged reference.
        # That is cold start plus sampling noise with no motion, and the
        # excess over it is what the motion adds.
        cold = {}
        for frame in motion:
            held = engine.sequence(f"{gate}_{rung}_held_f{frame}", rung, ctx.tier, [frame], frame + 4,
                                   extra + ["--bench-camera-hold-at", str(frame)], view_mode=view, seed=seed,
                                   debug_flags=debug_flags)
            cold[frame] = temporal_error(load_exr(held[frame][0]), parked[frame], mask_for(parked[frame]))
        # The excess over the hold control, which the Q6 budgets are written
        # against (docs/audits/MLIGHT_thresholds.tsv): the value minus the
        # control's own error at the same frame.
        excess: dict[int, tuple[float, float]] = {}
        for frame in motion + post:
            image = load_exr(moving[frame][0])
            reference = parked[parked_at[frame]]
            phase = "during" if frame <= stop else f"post_stop_{frame - stop}"
            control = cold.get(frame, post_controls.get(frame, floor_e))
            value = temporal_error(image, reference, mask_for(reference))
            rows.append((Row(gate, f"{rung}-{phase}", f"frame_{frame}", "temporal_error",
                             value, control, 0.0, passed="report"), ""))
            excess[frame] = (value - control, control)
        rows += temporal_excess_rows(gate, excess, motion, stop)
    for row, _ in rows:
        row.temporal_seed = seed
    return rows


# Frames before this carry the run's cold start and are reported, not gated.
A14_GATED_FROM_FRAME = 80
# The post-stop sample the A3 settle budget reads: stop + 16.
A3_SETTLE_OFFSET = 16


def temporal_excess_rows(gate: str, excess: dict[int, tuple[float, float]], motion: list[int], stop: int):
    """The rows the threshold table gates for A3 and A14: the worst excess over
    the hold control during motion, and (A3) the excess once settled."""
    def worst(frames: list[int]):
        frame = max(frames, key=lambda f: excess[f][0])
        return excess[frame]

    def row(case: str, region: str, metric: str, pick):
        value, control = pick
        return (Row(gate, case, region, metric, value, control, 0.0, passed="report"), "")

    if gate == "A3":
        return [
            row("penumbra-direct-pan", "during_motion", "excess_over_hold_max", worst(motion)),
            row("penumbra-direct-pan", "post_stop_16", "excess_over_hold", excess[stop + A3_SETTLE_OFFSET]),
        ]
    if gate == "A14":
        gated = [f for f in motion if f >= A14_GATED_FROM_FRAME]
        return [row("penumbra-moving", "from_frame_80", "excess_over_hold_max", worst(gated))]
    return []


def measure_a3_seed_floor(primary_rows: list[tuple[Row, str]], paired_rows: list[tuple[Row, str]]) -> None:
    """Measure how much A3's per-frame excess changes with the RT seed."""
    def excess_by_frame(rows: list[tuple[Row, str]]) -> dict[str, Row]:
        return {row.region: row for row, _ in rows if row.gate == "A3" and row.metric == "temporal_error"}

    primary = excess_by_frame(primary_rows)
    paired = excess_by_frame(paired_rows)
    for row, _ in primary_rows:
        if row.gate != "A3" or row.case != "penumbra-direct-pan":
            continue
        phase = "during" if row.region == "during_motion" else row.region
        frames = [sample for sample in primary.values() if sample.case == f"penumbra-{phase}"]
        # The threshold applies to both seeds. The floor tracks their
        # disagreement at the same frame rather than a difference of maxima.
        row.value = max(row.value, *(paired[sample.region].value - paired[sample.region].floor_engine
                                     for sample in frames))
        # The per-frame floor_engine column holds the matched control error.
        # Subtract it within each seed before comparing the two excesses.
        row.floor_engine = max(
            abs((sample.value - sample.floor_engine)
                - (paired[sample.region].value - paired[sample.region].floor_engine))
            for sample in frames
        )
        row.temporal_floor_seed = paired[frames[0].region].temporal_seed


# ── Threshold table (RLT-REF-006, Q6) ────────────────────────────────────

THRESHOLD_TABLE = REPO / "docs" / "audits" / "MLIGHT_thresholds.tsv"

# A floor of exactly zero (a band whose two seeded captures agree bit for bit)
# would fail any drift margin proportional to it. This is the smallest floor
# increase the drift check treats as a rise, well under every budget in the
# table (the smallest is 5e-4).
DRIFT_EPSILON = 1e-6


def load_thresholds(path: Path = THRESHOLD_TABLE) -> dict[tuple[str, str, str, str, str], dict[str, str]]:
    """The owner's frozen floors and budgets, keyed by gate, rung, region,
    metric and tier. Comment lines start with `#`; the first other line is the
    column header."""
    table: dict[tuple[str, str, str, str, str], dict[str, str]] = {}
    header: list[str] | None = None
    for line in path.read_text().splitlines():
        if not line.strip() or line.startswith("#"):
            continue
        fields = line.split("\t")
        if header is None:
            header = fields
            continue
        record = dict(zip(header, fields))
        table[(record["gate"], record["rung"], record["region"], record["metric"], record["tier"])] = record
    return table


def threshold_key(row: Row, tier: int) -> tuple[str, str, str, str, str]:
    """A manifest row's table key. A5 names its tier in the case
    (`filtered_direct-tier3`); every other gate runs at the context's tier."""
    case, row_tier = row.case, str(tier)
    base, sep, suffix = case.rpartition("-tier")
    if sep and suffix.isdigit():
        case, row_tier = base, suffix
    return (row.gate, case, row.region, row.metric, row_tier)


def apply_thresholds(rows: list[tuple[Row, str]], tier: int,
                     table: dict[tuple[str, str, str, str, str], dict[str, str]]) -> None:
    """Judge every row the table names: the metric within its threshold, and
    the re-measured floor within `frozen_floor + drift margin` (RLT-REF-006).
    The margin is `2x frozen floor` throughout, so the floor may reach
    3x the frozen one (plus DRIFT_EPSILON). Rows the table does not name stay
    `report`."""
    for row, _ in rows:
        if row.gate == "A3" and row.temporal_floor_seed is None:
            continue
        record = table.get(threshold_key(row, tier))
        if record is None:
            continue
        threshold = float(record["threshold"])
        frozen = float(record["frozen_floor"])
        row.frozen_floor = f"{frozen:.3g}"
        row.threshold = f"<={threshold:.4g} and floor<={3 * frozen + DRIFT_EPSILON:.3g}"
        within = row.value <= threshold
        steady = row.floor_combined <= 3 * frozen + DRIFT_EPSILON
        row.passed = "yes" if within and steady else "no"


# ── Driver ───────────────────────────────────────────────────────────────


def git(*args: str) -> str:
    return subprocess.run(["git", "-C", str(REPO), *args], capture_output=True, text=True).stdout.strip()


def gpu_identity() -> str:
    try:
        out = subprocess.run(["vulkaninfo", "--summary"], capture_output=True, text=True, timeout=60).stdout
    except (OSError, subprocess.SubprocessError):
        return "unknown"
    name = next((l.split("=", 1)[1].strip() for l in out.splitlines() if "deviceName" in l), "unknown")
    driver = next((l.split("=", 1)[1].strip() for l in out.splitlines() if "driverInfo" in l), "")
    return f"{name} {driver}".strip()


def main() -> int:
    parser = argparse.ArgumentParser(description=(__doc__ or "").split("\n\n")[0])
    parser.add_argument("--out", type=Path, default=REPO / "target" / "m-light")
    parser.add_argument("--cache", type=Path, default=None,
                        help="reference cache (default: <out>/reference-cache)")
    parser.add_argument("--gates", default="A15,A16")
    parser.add_argument("--coverage-scenes", default="",
                        help="RCG-COV: comma-separated manifest scenes (default: all)")
    parser.add_argument("--tier", type=int, default=3)
    parser.add_argument("--spp", type=int, default=1024)
    parser.add_argument("--frames", type=int, default=64, help="engine frames averaged per capture")
    parser.add_argument("--warm-frames", type=int, default=20)
    parser.add_argument("--temporal-seed", type=int, default=0,
                        help="sampling seed offset for A3/A14 temporal captures")
    parser.add_argument("--temporal-debug-flags", default="0",
                        help="renderer debug mask for temporal attribution (diagnostic, not an acceptance run)")
    parser.add_argument("--profile", default="dev", choices=("dev", "release"))
    parser.add_argument("--cold", action="store_true", help="empty the reference cache first (A18)")
    args = parser.parse_args()
    try:
        temporal_mask = int(args.temporal_debug_flags, 0)
    except ValueError:
        parser.error("--temporal-debug-flags must be a decimal or 0x-prefixed integer")
    if not 0 <= temporal_mask <= 0xFFFFFFFF:
        parser.error("--temporal-debug-flags must fit an unsigned 32-bit mask")

    t_start = time.monotonic()
    out: Path = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    cache = args.cache or out / "reference-cache"
    if args.cold and cache.exists():
        shutil.rmtree(cache)

    log(f"building engine ({args.profile})")
    cargo = ["cargo", "build", "-p", "byroredux", "--bin", "byroredux"]
    if args.profile == "release":
        cargo.append("--release")
    subprocess.run(cargo, cwd=REPO, check=True)
    binary = REPO / "target" / ("release" if args.profile == "release" else "debug") / "byroredux"

    variant = pick_variant()
    store = ReferenceStore(cache, variant)
    engine = Engine(binary, out, dict(os.environ), args.warm_frames, args.frames)
    ctx = Context(
        revision=git("rev-parse", "--short=12", "HEAD") + ("-dirty" if git("status", "--porcelain") else ""),
        gpu=gpu_identity(), tier=args.tier, engine_frames=args.frames, engine_seeds="0,1",
        reference_spp=args.spp, reference_seeds="0,1",
    )
    log(f"Mitsuba {mi.__version__} ({variant}), revision {ctx.revision}, GPU {ctx.gpu}")

    rows: list[tuple[Row, str]] = []
    gates = {g.strip() for g in args.gates.split(",") if g.strip()}
    if "A15" in gates:
        rows += gate_calibration(engine, store, ctx, args.spp)
        rows += gate_materials(engine, store, ctx, args.spp)
    if "A16" in gates:
        rows += gate_closure(engine, store, ctx, args.spp)
    if "A7" in gates:
        rows += gate_gi_deficit(engine, store, ctx, args.spp)
    if "A8" in gates:
        rows += gate_traced_share_ao(engine, store, ctx, args.spp)
    if "RCG-FLAT" in gates:
        rows += gate_flatness(engine, ctx)
    if "RCG-COV" in gates:
        rows += gate_coverage(engine, ctx, {s for s in args.coverage_scenes.split(",") if s} or None)
    if gates & {"A2", "A4"}:
        rows += gate_penumbra(engine, store, ctx, args.spp, gates)
    if "A5" in gates:
        rows += gate_contact(engine, store, ctx, args.spp)
    if "CON-004" in gates:
        rows += gate_far_sun(engine, ctx)
    if "A6" in gates:
        rows += gate_sky_ibl(engine, store, ctx, args.spp)
    if gates & {"A3", "A14", "SHD-006"}:
        rows += gate_temporal(engine, store, ctx, args.spp, gates, args.temporal_seed, args.temporal_debug_flags)
    a3_floor_seed = None
    if "A3" in gates and temporal_mask == 0:
        a3_floor_seed = args.temporal_seed ^ 1
        paired = gate_temporal(engine, store, ctx, args.spp, {"A3"}, a3_floor_seed)
        measure_a3_seed_floor(rows, paired)
        # Preserve both seeds' per-frame moving and control errors.
        rows += [(row, key) for row, key in paired if row.metric == "temporal_error"]
    if "SHD-005" in gates:
        rows += gate_reservoir_clamp(engine, store, ctx, args.spp)

    # Debug-mask attribution deliberately changes the renderer; never judge
    # those rows against thresholds frozen for normal rendering.
    judged_rows = [(row, key) for row, key in rows
                   if row.gate not in {"A3", "A14", "SHD-006", "SHD-006-BASE"}
                   or temporal_mask == 0]
    apply_thresholds(judged_rows, ctx.tier, load_thresholds())
    manifest = out / "manifest.tsv"
    write_manifest(manifest, rows, ctx)
    wall = time.monotonic() - t_start
    with open(out / "run-metadata.txt", "w") as meta:
        meta.write(f"revision={ctx.revision}\ngpu={ctx.gpu}\nmitsuba={mi.__version__}\nvariant={variant}\n")
        meta.write(f"tier={ctx.tier}\nengine_frames={ctx.engine_frames}\nreference_spp={ctx.reference_spp}\n")
        meta.write(f"temporal_seed={args.temporal_seed}\ntemporal_debug_flags={args.temporal_debug_flags}\n")
        if a3_floor_seed is not None:
            meta.write(f"a3_floor_seed={a3_floor_seed}\n")
        meta.write(f"cache={'cold' if args.cold else 'warm'}\nreferences_rendered={store.timings['rendered']}\n")
        meta.write(f"references_cached={store.timings['cached']}\n")
        meta.write(f"reference_render_seconds={store.timings['seconds']:.1f}\nwall_seconds={wall:.1f}\n")

    failed = [r for r, _ in rows if r.passed == "no"]
    skipped = [r for r, _ in rows if r.passed == "skip"]
    for row, _ in rows:
        log(f"{row.gate} {row.case:24s} {row.region:18s} {row.metric:22s} {row.value:+.6f} "
            f"floor {row.floor_combined:.2e} {row.passed}")
    # A18 (RLT-REF-016): the run time is printed at the end of every run.
    log(f"gate run time {wall:.1f} s ({'cold' if args.cold else 'warm'} cache: "
        f"{store.timings['rendered']} rendered in {store.timings['seconds']:.1f} s, "
        f"{store.timings['cached']} cached)")
    log(f"manifest: {manifest}")
    if skipped and len(skipped) == len(rows):
        # RCG-VAL-004: nothing was measured, so nothing may read as a pass.
        log(f"SKIP: the data for {len(skipped)} row(s) is absent (exit {EXIT_SKIP})")
        return EXIT_SKIP
    if failed:
        log(f"{len(failed)} gate row(s) failed")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
