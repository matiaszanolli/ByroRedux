#!/usr/bin/env python3
"""M-LIGHT reference gates (docs/prd/reference-lighting.md).

Runs the engine's Cornell reference rungs, exports each scene to Mitsuba 3,
renders the converged reference (cached, RLT-REF-015), and computes the
Cornell gates on linear captures (RLT-REF-007):

  A2   penumbra width             (RLT-SHD-001; report until the L1 budget)
  A4   filter bands and thin rod  (RLT-SHD-004; report until the L1 budget)
  A3   penumbra under camera pan  (RLT-SHD-002; report until the L1 budgets)
  A5   contact bands              (RLT-CON-001…008; report until the L2 budget)
  A14  moving blocker             (RLT-SHD-010; report until the L1 budgets)
  A7   GI deficit                 (RLT-GI-002; report until Q5's cache)
  A8   traced share free of SSAO  (RLT-GI-004)
  A15  calibration and materials  (RLT-REF-010, RLT-REF-014)
  A16  closure                    (RLT-REF-011, RLT-REF-018, RLT-REF-020)
  A18  apparatus cost             (RLT-REF-016)

Every metric is written to manifest.tsv with its noise floor on both sides
(RLT-REF-004, RLT-REF-006): each side is captured twice with different
sampling seeds (`--rt-test-seed-offset` on the engine, the sampler seed on
Mitsuba) and the combined floor is the root-sum-square of the two sides.
No factor is ever applied to an image; emitters are scaled at export
(k_exp, docs/engine/path-tracing.md §2.1).

Needs a Python with Mitsuba 3 and NumPy (a developer tool, never an engine
dependency). `scripts/m-light-gates.sh` picks the interpreter.
"""

from __future__ import annotations

import argparse
import hashlib
import math
import os
import shutil
import subprocess
import sys
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
    return h.hexdigest()


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
        image = mi.render(scene, spp=spp, seed=seed)
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
    neither the XML order nor `Scene.shapes()`.) Positions are world units,
    shape (height, width, 3).
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
    positions = np.array(si.p).T.reshape(height, width, 3)
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
            values = [
                row.gate, row.case, row.region, row.metric, f"{row.value:.6g}",
                f"{row.floor_engine:.3g}", f"{row.floor_reference:.3g}", f"{row.floor_combined:.3g}",
                "", row.threshold, row.passed, str(ctx.tier), str(ctx.engine_frames),
                ctx.engine_seeds, str(ctx.reference_spp), ctx.reference_seeds, ctx.k_exp, key,
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
# (A15): the point from Phase 1a, the sun from Phase 3.
CALIBRATION_RUNGS = ("cal-point", "cal-sun")


def gate_calibration(engine: Engine, store: ReferenceStore, ctx: Context, spp: int):
    """A15, per emitter kind (RLT-REF-010): direct_only over the plane."""
    rows = []
    for rung in CALIBRATION_RUNGS:
        rows += calibration_rows(engine, store, ctx, spp, rung)
    return rows


def calibration_rows(engine: Engine, store: ReferenceStore, ctx: Context, spp: int, rung: str):
    rows = []
    export = engine.out / "scenes" / rung
    name = rung.replace("-", "_") + "_direct"
    ea = engine.capture(name, rung, "direct_only", 0, ctx.tier, export_dir=export)
    eb = engine.capture(name, rung, "direct_only", 1, ctx.tier)
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


# ── Penumbra: A2, A4 (Phase 3) ───────────────────────────────────────────

PENUMBRA_RUNGS = ("penumbra", "penumbra-sun")
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
    extra = ["--rt-test-no-jitter"]
    for rung_name in PENUMBRA_RUNGS:
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
        ref = [store.render(export, "direct", spp, s) for s in (0, 1)]
        ref_open = [store.render(opened, "direct", spp, s) for s in (0, 1)]
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
    return rows


# ── Contact: A5 (Phase 3) ────────────────────────────────────────────────

CONTACT_TIERS = (1, 2, 3)  # RLT-CON-001: tiers 1-3, an exception to Q7's tier-3 rule


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


def gate_contact(engine: Engine, store: ReferenceStore, ctx: Context, spp: int):
    """A5 contact bands (RLT-CON-001, -002, -006, -007, -008).

    Visibility (§2) per caster's contact band, the engine's filtered direct
    view (shadow filter on, RLT-CON-002) and `direct_only` (off) at tiers
    1-3, unjittered, against the reference's direct recipe over the same with
    the casters removed. Report-only until the owner sets the L2 budget.
    """
    del ctx
    rows = []
    rung_name = "contact"
    export = engine.out / "scenes" / rung_name
    extra = ["--rt-test-no-jitter"]
    exported = False
    reference = None
    for view_mode in ("filtered_direct", "direct_only"):
        for tier in CONTACT_TIERS:
            name = f"contact_{view_mode}_t{tier}"
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
                receiver_name = band[1]
                receiver = receivers[receiver_name]
                mask = band_mask(positions, receiver, band)
                ve = [visibility(shadowed[s], unshadowed[s], receiver, rung.min_unshadowed)[0] for s in (0, 1)]
                ref_v = vr[receiver_name]
                mean, p95 = visibility_error(ve[0], ref_v[0], mask)
                mean_e, p95_e = visibility_error(ve[1], ref_v[0], mask)
                mean_r, p95_r = visibility_error(ve[0], ref_v[1], mask)
                case = f"{view_mode}-tier{tier}"
                rows.append((Row("A5", case, band[0], "visibility_error_mean", mean,
                                 abs(mean_e - mean), abs(mean_r - mean), passed="report"), key))
                rows.append((Row("A5", case, band[0], "visibility_error_p95", p95,
                                 abs(p95_e - p95), abs(p95_r - p95), passed="report"), key))
                if tier == CONTACT_TIERS[-1] and view_mode == "filtered_direct":
                    rows.append((Row("A5", case, band[0], "band_pixels", float(mask.sum()), 0.0, 0.0,
                                     passed="report"), key))
    rows += contact_mid_animation(engine, store, spp)
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
    # renderer-stepped needs a non-static path; parked at frame 0 it holds
    # the rung's camera while the animation clock steps.
    base = ["--rt-test-no-jitter", "--upscaler", "taa",
            "--bench-camera", "pan", "--bench-camera-freeze-at", "0"]
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


def gate_temporal(engine: Engine, store: ReferenceStore, ctx: Context, spp: int, gates: set[str]):
    """A14 moving blocker (RLT-SHD-010) and A3 camera pan (RLT-SHD-002):
    the engine's `final` during and after a motion against itself parked and
    converged at the same frame of the same path. Report-only until the L1
    budgets. No reference: both sides carry the same indirect bias."""
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
        cases.append(("A14", "penumbra-moving", clock))
    if "A3" in gates:
        cases.append(("A3", "penumbra", clock + ["--bench-camera", "pan"]))
    if "SHD-006" in gates:
        # RLT-SHD-006: the A3 capture again with TAA's variance clip at 1.0
        # beside the shipped 1.5, shadow filter on.
        cases.append(("SHD-006", "penumbra", clock + ["--bench-camera", "pan", "--rt-test-taa-gamma", "1.0"]))
    for gate, rung, extra in cases:
        export = engine.out / "scenes" / f"{rung}-temporal"
        # The receiver in view at the rung's own (start) pose; the pan's
        # later poses see a rotated view of the same floor, so its mask is
        # taken from the parked capture's non-black floor pixels instead.
        engine.capture(f"{gate}_export", rung, "final", 0, ctx.tier, export_dir=export)
        masks = region_masks(export)
        floor = masks["pen_floor"]
        moving = engine.sequence(f"{gate}_{rung}_moving", rung, ctx.tier, motion + post, bench_frames, extra)
        parked_at = {f: f for f in motion} | {f: stop for f in post}
        parked = {}
        for frame in sorted(set(parked_at.values())):
            parked[frame] = load_exr(engine.capture(
                f"{gate}_{rung}_parked_f{frame}", rung, "final", 0, ctx.tier,
                extra=extra + ["--bench-camera-freeze-at", str(frame)],
                bench_frames=frame + PARKED_SETTLE, bench_mode="renderer-stepped"))
        # The floor: the same single-frame captures with the path parked at
        # the stop from frame 0, against the same converged reference — what
        # per-frame sampling noise alone contributes.
        still = engine.sequence(f"{gate}_{rung}_still", rung, ctx.tier, post, bench_frames,
                                extra + ["--bench-camera-freeze-at", str(stop)])

        def mask_for(reference):
            return floor if gate == "A14" else erode(luminance(reference) > 1e-4, ERODE_PX)

        floor_e = float(np.mean([temporal_error(load_exr(still[f][0]), parked[stop], mask_for(parked[stop]))
                                 for f in post]))
        for frame in motion + post:
            image = load_exr(moving[frame][0])
            reference = parked[parked_at[frame]]
            phase = "during" if frame <= stop else f"post_stop_{frame - stop}"
            row = Row(gate, f"{rung}-{phase}", f"frame_{frame}", "temporal_error",
                      temporal_error(image, reference, mask_for(reference)), floor_e, 0.0, passed="report")
            rows.append((row, ""))
    return rows


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
    parser.add_argument("--tier", type=int, default=3)
    parser.add_argument("--spp", type=int, default=1024)
    parser.add_argument("--frames", type=int, default=64, help="engine frames averaged per capture")
    parser.add_argument("--warm-frames", type=int, default=20)
    parser.add_argument("--profile", default="dev", choices=("dev", "release"))
    parser.add_argument("--cold", action="store_true", help="empty the reference cache first (A18)")
    args = parser.parse_args()

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
    if gates & {"A2", "A4"}:
        rows += gate_penumbra(engine, store, ctx, args.spp, gates)
    if "A5" in gates:
        rows += gate_contact(engine, store, ctx, args.spp)
    if gates & {"A3", "A14", "SHD-006"}:
        rows += gate_temporal(engine, store, ctx, args.spp, gates)
    if "SHD-005" in gates:
        rows += gate_reservoir_clamp(engine, store, ctx, args.spp)

    manifest = out / "manifest.tsv"
    write_manifest(manifest, rows, ctx)
    wall = time.monotonic() - t_start
    with open(out / "run-metadata.txt", "w") as meta:
        meta.write(f"revision={ctx.revision}\ngpu={ctx.gpu}\nmitsuba={mi.__version__}\nvariant={variant}\n")
        meta.write(f"tier={ctx.tier}\nengine_frames={ctx.engine_frames}\nreference_spp={ctx.reference_spp}\n")
        meta.write(f"cache={'cold' if args.cold else 'warm'}\nreferences_rendered={store.timings['rendered']}\n")
        meta.write(f"references_cached={store.timings['cached']}\n")
        meta.write(f"reference_render_seconds={store.timings['seconds']:.1f}\nwall_seconds={wall:.1f}\n")

    failed = [r for r, _ in rows if r.passed == "no"]
    for row, _ in rows:
        log(f"{row.gate} {row.case:24s} {row.region:18s} {row.metric:22s} {row.value:+.6f} "
            f"floor {row.floor_combined:.2e} {row.passed}")
    # A18 (RLT-REF-016): the run time is printed at the end of every run.
    log(f"gate run time {wall:.1f} s ({'cold' if args.cold else 'warm'} cache: "
        f"{store.timings['rendered']} rendered in {store.timings['seconds']:.1f} s, "
        f"{store.timings['cached']} cached)")
    log(f"manifest: {manifest}")
    if failed:
        log(f"{len(failed)} gate row(s) failed")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
