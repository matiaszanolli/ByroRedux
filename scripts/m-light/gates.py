#!/usr/bin/env python3
"""M-LIGHT reference gates (docs/prd/reference-lighting.md).

Runs the engine's Cornell reference rungs, exports each scene to Mitsuba 3,
renders the converged reference (cached, RLT-REF-015), and computes the
Phase 1a gates on linear captures (RLT-REF-007):

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


def region_masks(scene_dir: Path, erode_px: int = ERODE_PX) -> dict[str, np.ndarray]:
    """Per-shape pixel masks, eroded by ERODE_PX.

    One camera ray per pixel centre — the point the engine shades — is
    intersected with the exported scene and the hit shape compared with each
    shape. (Mitsuba's `shape_index` AOV is not used: its indices follow
    neither the XML order nor `Scene.shapes()`.)
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
    hit = scene.ray_intersect(rays).shape
    masks = {}
    for shape in scene.shapes():
        mask = np.array(hit == mi.ShapePtr(shape)).reshape(height, width)
        masks[shape.id()] = erode(mask, erode_px)
    return masks


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
                export_dir: Path | None = None, reference_view: int = 0) -> Path:
        exr = self.out / "engine" / f"{name}_s{seed}.exr"
        exr.parent.mkdir(parents=True, exist_ok=True)
        args = [
            str(self.binary), "--cornell-reference", rung,
            "--cornell-reference-view", str(reference_view),
            "--render-debug-mode", view_mode,
            "--bench-frames", str(self.frames_warm), "--bench-mode", "renderer-static",
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


def gate_calibration(engine: Engine, store: ReferenceStore, ctx: Context, spp: int):
    """A15, point emitter (RLT-REF-010): direct_only over the plane."""
    rows = []
    export = engine.out / "scenes" / "cal-point"
    ea = engine.capture("cal_point_direct", "cal-point", "direct_only", 0, ctx.tier, export_dir=export)
    eb = engine.capture("cal_point_direct", "cal-point", "direct_only", 1, ctx.tier)
    ra, key = store.render(export, "direct", spp, 0)
    rb, _ = store.render(export, "direct", spp, 1)
    mask = region_masks(export)["cal_plane"]
    ea, eb = load_exr(ea), load_exr(eb)
    rows.append((unit_ratio_row("A15", "cal-point", "cal_plane", ea, eb, ra, rb, mask), key))
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
            rows.append((unit_ratio_row("A15", "cal-point", f"cal_plane_{j}{i}", ea, eb, ra, rb, sub), key))
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
