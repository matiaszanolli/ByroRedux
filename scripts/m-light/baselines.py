#!/usr/bin/env python3
"""M-LIGHT Phase 1b global baselines (docs/prd/reference-lighting.md §9).

For every benchmark scene (the ROADMAP bench table: Cornell, Prospector,
WhiterunBanneredMare, MedTekResearch01, DmndDugoutInn01; scene arguments
from scripts/bench-scenes.sh) this records:

  A13  the settled ray-quality tier and its oscillation (RLT-PERF-005): one
       adaptive run under the bench-of-record conditions (renderer-stepped,
       frame-indexed orbit camera, TAA at native resolution, a fresh settings
       file), its `rt_tier_hist` past the engine's fixed warm-up; then the
       pinned-tier frame time RLT-PERF-004 / -006 measure against: `--runs`
       runs with `--rt-test-ray-quality-tier` at the settled tier, median wall
       time and GPU brackets;
  A11  the content anchor (RLT-CNT-001): two seeded linear captures of the
       `final` view at the pinned pose (the adaptive run's end-of-path camera)
       with no jitter, and their 32 x 32-tile two-capture floor;
  A9   the material-class dump with the RLT-IBL-008 flags (Wave 1 game scenes).

Writes <out>/baselines.tsv, <out>/materials.tsv, <out>/anchors/*.exr and
<out>/run-metadata.txt. Needs a Python with Mitsuba 3 and NumPy for reading
EXR (scripts/m-light-baselines.sh picks it); the engine is built --release,
as the bench of record is.
"""

from __future__ import annotations

import argparse
import hashlib
import os
import re
import statistics
import subprocess
import sys
import tempfile
import time
from pathlib import Path

import numpy as np
import mitsuba as mi

REPO = Path(__file__).resolve().parents[2]
GAMES_ROOT = Path(os.environ.get("BYROREDUX_GAMES_ROOT", "/mnt/data/SteamLibrary/steamapps/common"))
SCENES = ("cornell", "prospector", "whiterun", "medtek", "dugout")
WAVE1_GAME_SCENES = ("prospector", "whiterun", "medtek", "dugout")
LUMA = np.array([0.2126, 0.7152, 0.0722])
TILE = 32  # RLT-CNT-001 tile grid
# The bench of record's reference (TAA at native resolution) and the shipping
# default (FSR 3.1 Quality, which renders fewer pixels and can settle on a
# higher tier). The settled tier a player sees is the second one's.
CONFIGS = (
    ("taa", ["--upscaler", "taa"]),
    ("fsr-quality", ["--upscaler", "fsr3", "--fsr-quality", "quality"]),
)

# RLT-IBL-008 thresholds, fixed here and recorded in the manifest.
F1_ROUGHNESS = 0.2
F3_TOLERANCE = 1e-3
NEAR_MIRROR_ROUGHNESS_FLOOR = 0.04  # merge.rs: (1 - smoothness).clamp(0.04, 1)
BGSM_NO_GLOSS_FALLBACK = 0.5        # merge.rs / material_translate.rs fallback


def log(message: str) -> None:
    print(f"m-light-baselines: {message}", flush=True)


def scene_spec(scene: str) -> tuple[Path, list[str], int]:
    """(working dir, load args, entity floor) from scripts/bench-scenes.sh."""
    script = (
        'source "$REPO/scripts/bench-scenes.sh"; run_scene_args "$1"; '
        'printf "%s\\n" "$(scene_dir "$1")" "$(scene_entity_floor "$1")"; printf "%s\\0" "${ARGS[@]}"'
    )
    out = subprocess.run(
        ["bash", "-c", script, "bench-scenes", scene],
        env=dict(os.environ, REPO=str(REPO), GAMES_ROOT=str(GAMES_ROOT)),
        capture_output=True, check=True,
    ).stdout
    head, _, args_blob = out.partition(b"\n")
    floor_line, _, args_blob = args_blob.partition(b"\n")
    args = [a.decode() for a in args_blob.split(b"\0") if a]
    return Path(head.decode()), args, int(floor_line)


def bench_fields(line: str) -> dict[str, str]:
    return dict(re.findall(r"(\w+)=(\S+)", line))


def gpu_brackets(line: str) -> dict[str, float]:
    # `gpu_ms=[main=… svgf=… …]`-style tokens; read every `gpu_<pass>=` /
    # `<pass>=<float>` inside the bracketed GPU list conservatively.
    out = {}
    for key, value in re.findall(r"gpu_(\w+?)=([0-9.]+)", line):
        out[key] = float(value)
    return out


class Engine:
    def __init__(self, binary: Path, out: Path):
        self.binary = binary
        self.out = out

    def run(self, scene: str, name: str, extra: list[str]) -> tuple[str, Path]:
        workdir, args, floor = scene_spec(scene)
        log_path = self.out / "logs" / f"{scene}_{name}.log"
        log_path.parent.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory() as settings_dir:
            env = dict(os.environ, RUST_LOG="warn",
                       BYROREDUX_SETTINGS_PATH=str(Path(settings_dir) / "settings.toml"))
            with open(log_path, "w") as log_file:
                subprocess.run([str(self.binary), *args, *extra], cwd=workdir, env=env,
                               stdout=log_file, stderr=subprocess.STDOUT, timeout=1800)
        text = log_path.read_text(errors="replace")
        lines = [l for l in text.splitlines() if l.startswith("bench:")]
        if not lines:
            raise SystemExit(f"m-light-baselines: {scene}/{name} printed no bench line; see {log_path}")
        if re.search(r"was specified but 0 .* archives opened", text):
            raise SystemExit(f"m-light-baselines: {scene}/{name}: archives failed to open; see {log_path}")
        line = lines[-1]
        entities = int(bench_fields(line).get("entities", "0"))
        if entities < floor:
            raise SystemExit(f"m-light-baselines: {scene}/{name}: {entities} entities < floor {floor}")
        return line, log_path


def tile_stats(a: np.ndarray, b: np.ndarray) -> tuple[float, float]:
    """Mean and 95th percentile over 32x32 tiles of |mean_a - mean_b| / mean."""
    la, lb = a @ LUMA, b @ LUMA
    h, w = la.shape
    values = []
    for y in range(0, h - TILE + 1, TILE):
        for x in range(0, w - TILE + 1, TILE):
            ma = la[y:y + TILE, x:x + TILE].mean()
            mb = lb[y:y + TILE, x:x + TILE].mean()
            base = max((ma + mb) * 0.5, 1e-4)
            values.append(abs(ma - mb) / base)
    return float(np.mean(values)), float(np.percentile(values, 95))


def material_report(dump: Path, scene: str) -> list[list[str]]:
    rows = []
    header, *body = dump.read_text().splitlines()
    cols = header.split("\t")
    records = [dict(zip(cols, line.split("\t"))) for line in body]
    classes: dict[tuple[str, str], list[tuple[float, float, int]]] = {}
    f1_rough = f1_total = 0
    f2 = 0
    f3 = f3_total = 0
    for r in records:
        n = int(r["instances"])
        rough, metal = float(r["roughness"]), float(r["metalness"])
        classes.setdefault((r["route"], r["conductor"]), []).append((rough, metal, n))
        if r["conductor"] == "dielectric" and r["material_kind"] == "0":
            f1_total += n
            if rough < F1_ROUGHNESS:
                f1_rough += n
        # F2: a conductor whose route carries no metal signal — every legacy
        # route except the environment-map arm with authored specular.
        if r["conductor"] == "conductor" and r["route"] not in ("bgsm", "legacy_env_map"):
            f2 += n
        # F3 (FO4 BGSM): roughness against the engine's own rule, the
        # unverified convention of #6606; the 0.5 no-gloss fallback excluded.
        if r["route"] == "bgsm" and scene in ("medtek", "dugout") and abs(rough - BGSM_NO_GLOSS_FALLBACK) > 1e-6:
            f3_total += n
            expected = min(max(1.0 - float(r["glossiness"]) / 100.0, NEAR_MIRROR_ROUGHNESS_FLOOR), 1.0)
            if abs(rough - expected) > F3_TOLERANCE:
                f3 += n
    for (route, conductor), values in sorted(classes.items()):
        weights = np.array([v[2] for v in values], dtype=float)
        rough = np.repeat([v[0] for v in values], weights.astype(int))
        metal = np.repeat([v[1] for v in values], weights.astype(int))
        rows.append([scene, route, conductor, str(int(weights.sum())),
                     f"{np.percentile(rough, 5):.3f}", f"{np.median(rough):.3f}", f"{np.percentile(rough, 95):.3f}",
                     f"{np.median(metal):.3f}", "", "", ""])
    rows.append([scene, "FLAGS", "", str(sum(int(r["instances"]) for r in records)), "", "", "", "",
                 f"F1={f1_rough}/{f1_total} (roughness<{F1_ROUGHNESS})",
                 f"F2={f2} (conductor without metal signal)",
                 f"F3={f3}/{f3_total} (FO4 BGSM vs (1-s).clamp(0.04,1); unverified, #6606)"])
    return rows


def capture_anchor_pair(engine: Engine, scene: str, out: Path, tier: int, pos: str, fwd: str,
                        frames: int) -> list[Path]:
    """Two seeded linear captures of `final` at a pinned pose (the A11 anchor)."""
    anchors = []
    for seed in (0, 1):
        exr = out / "anchors" / f"{scene}_s{seed}.exr"
        exr.parent.mkdir(parents=True, exist_ok=True)
        engine.run(scene, f"anchor_s{seed}", [
            "--upscaler", "taa", "--rt-test-no-jitter", "--rt-test-ray-quality-tier", str(tier),
            "--rt-test-seed-offset", str(seed), "--camera-pos", pos, "--camera-forward", fwd,
            "--bench-frames", "30", "--bench-mode", "renderer-static",
            "--linear-capture", str(exr), "--linear-capture-frames", str(frames)])
        anchors.append(exr)
    return anchors


def capture_anchors_only(engine: Engine, args, out: Path) -> int:
    header, *rows = args.anchors_only.read_text().splitlines()
    cols = header.split("\t")
    for line in rows:
        r = dict(zip(cols, line.split("\t")))
        if r.get("config", "taa") != "taa":
            continue
        scene = r["scene"]
        anchors = capture_anchor_pair(engine, scene, out, int(r["settled_tier"]), r["anchor_camera_pos"],
                                      r["anchor_camera_forward"], args.anchor_frames)
        log(f"{scene}: anchors {anchors[0].name}, {anchors[1].name} (tier {r['settled_tier']})")
    return 0


def exit_cost(engine: Engine, control: Engine, args, out: Path) -> int:
    """RLT-PERF-003..005 at a phase exit, against the same-machine control."""
    configs = dict(CONFIGS)
    header, *rows = args.exit_cost.read_text().splitlines()
    cols = header.split("\t")
    stepped = ["--bench-frames", str(args.frames), "--bench-mode", "renderer-stepped", "--bench-camera", "orbit"]
    report, failed = [], 0
    for line in rows:
        r = dict(zip(cols, line.split("\t")))
        scene, config, tier = r["scene"], r["config"], int(r["settled_tier"])
        base = configs[config] + stepped
        head_ms, control_ms = [], []
        for run in range(1, args.runs + 1):
            pinned = base + ["--rt-test-ray-quality-tier", str(tier)]
            control_ms.append(float(bench_fields(control.run(scene, f"{config}_control_{run}", pinned)[0])["wall_ms"]))
            head_ms.append(float(bench_fields(engine.run(scene, f"{config}_head_{run}", pinned)[0])["wall_ms"]))
        adaptive = [a if a != str(args.frames) else str(240 + args.window) for a in base]
        hist = [int(v) for v in bench_fields(engine.run(scene, f"{config}_adaptive", adaptive)[0])["rt_tier_hist"].split("/")]
        settled = int(np.argmax(hist))
        delta = statistics.median(head_ms) - statistics.median(control_ms)
        ceiling = max(0.05 * float(r["pinned_wall_ms_median"]), 0.25)  # Q8 per phase
        game_scene = scene != "cornell"  # Cornell is reported, not held (RLT-PERF-004)
        cost_ok = delta <= ceiling or not game_scene
        tier_ok = settled >= tier
        failed += (not cost_ok) + (not tier_ok)
        report.append([scene, config, str(tier), f"{statistics.median(control_ms):.3f}",
                       f"{statistics.median(head_ms):.3f}", f"{delta:+.3f}", f"{ceiling:.3f}",
                       "yes" if cost_ok else "no", str(settled), "/".join(map(str, hist)),
                       "yes" if tier_ok else "no"])
        log(f"{scene}/{config}: pinned tier {tier}: control {statistics.median(control_ms):.2f} ms, "
            f"head {statistics.median(head_ms):.2f} ms, delta {delta:+.2f} (ceiling {ceiling:.2f}); "
            f"settled tier {settled} (was {tier})")
    with open(out / "exit-cost.tsv", "w") as f:
        f.write("scene\tconfig\tpinned_tier\tcontrol_ms\thead_ms\tdelta_ms\tceiling_ms\tcost_pass\t"
                "settled_tier\trt_tier_hist\ttier_pass\n")
        for row in report:
            f.write("\t".join(row) + "\n")
    return 1 if failed else 0


def main() -> int:
    parser = argparse.ArgumentParser(description=(__doc__ or "").split("\n\n")[0])
    parser.add_argument("--out", type=Path, default=REPO / "target" / "m-light-baselines")
    parser.add_argument("--scenes", default=",".join(SCENES))
    parser.add_argument("--runs", type=int, default=3)
    parser.add_argument("--frames", type=int, default=300,
                        help="frames per pinned-tier timing run (the bench-of-record length)")
    parser.add_argument("--window", type=int, default=300,
                        help="adaptive frames counted after the engine's 240-frame tier warm-up")
    parser.add_argument("--anchor-frames", type=int, default=64)
    parser.add_argument("--exit-cost", type=Path, metavar="BASELINES_TSV",
                        help="phase-exit A13: pinned-tier frame time of this build against --control-binary "
                             "(interleaved), the Q8 per-phase ceiling, and the settled tier (RLT-PERF-003..005)")
    parser.add_argument("--control-binary", type=Path,
                        help="the phase entry's engine, built in a worktree (same-machine control)")
    parser.add_argument("--anchors-only", type=Path, metavar="BASELINES_TSV",
                        help="re-capture only the content anchors, at the pose and TAA settled tier a "
                             "previous baselines.tsv recorded (RLT-CNT-001: a phase entry's anchors)")
    args = parser.parse_args()
    # Absolute: game scenes run with their Data folder as the working
    # directory, so a relative output path would land inside the install.
    out: Path = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    t0 = time.monotonic()

    log("building engine (release)")
    subprocess.run(["cargo", "build", "--release", "-p", "byroredux", "--bin", "byroredux"], cwd=REPO, check=True)
    engine = Engine(REPO / "target" / "release" / "byroredux", out)
    mi.set_variant("scalar_rgb")

    base_rows, material_rows, pending_rows = [], [], []
    if args.anchors_only:
        return capture_anchors_only(engine, args, out)
    if args.exit_cost:
        if not args.control_binary:
            raise SystemExit("m-light-baselines: --exit-cost needs --control-binary")
        return exit_cost(engine, Engine(args.control_binary.resolve(), out / "control"), args, out)
    for scene in [s.strip() for s in args.scenes.split(",") if s.strip()]:
        workdir, _, _ = scene_spec(scene)
        if not workdir.is_dir():
            log(f"skip {scene}: {workdir} not present")
            continue
        stepped = ["--bench-frames", str(args.frames), "--bench-mode", "renderer-stepped",
                   "--bench-camera", "orbit"]
        dump = out / "materials" / f"{scene}.tsv"
        anchor_tier, anchor_fields = None, None
        for config, config_args in CONFIGS:
            extra = config_args + stepped
            if config == "taa" and scene in WAVE1_GAME_SCENES:
                extra = extra + ["--material-dump", str(dump)]
            adaptive = [a if a != str(args.frames) else str(240 + args.window) for a in extra]
            line, _ = engine.run(scene, f"{config}_adaptive", adaptive)
            fields = bench_fields(line)
            hist = [int(v) for v in fields["rt_tier_hist"].split("/")]
            settled = int(np.argmax(hist))
            oscillation = 1.0 - hist[settled] / max(sum(hist), 1)
            log(f"{scene}/{config}: settled tier {settled}, oscillation {oscillation:.3f} (hist {hist})")
            walls, gpus = [], []
            for run in range(1, args.runs + 1):
                pinned, _ = engine.run(scene, f"{config}_pinned_t{settled}_{run}",
                                       config_args + stepped + ["--rt-test-ray-quality-tier", str(settled)])
                walls.append(float(bench_fields(pinned)["wall_ms"]))
                gpus.append(gpu_brackets(pinned))
            gpu_median = {k: statistics.median(g.get(k, 0.0) for g in gpus) for k in gpus[0]}
            log(f"{scene}/{config}: pinned tier {settled} wall {statistics.median(walls):.2f} ms "
                f"({min(walls):.2f}-{max(walls):.2f})")
            pending_rows.append([
                scene, config, str(settled), f"{oscillation:.4f}", fields["rt_tier_warmup"], str(sum(hist)),
                fields["rt_tier_hist"], f"{statistics.median(walls):.3f}", f"{min(walls):.3f}",
                f"{max(walls):.3f}", ";".join(f"{k}={v:.3f}" for k, v in sorted(gpu_median.items())),
                fields.get("entities", ""), fields.get("render_extent", ""),
            ])
            if config == "taa":
                anchor_tier, anchor_fields = settled, fields
        fields, settled = anchor_fields, anchor_tier

        # Content anchor at the pinned pose: the stepped path's last camera.
        pos, fwd = fields["camera_pos"], fields["camera_forward"]
        anchors = capture_anchor_pair(engine, scene, out, settled, pos, fwd, args.anchor_frames)
        a, b = (np.array(mi.Bitmap(str(p)), dtype=np.float64)[..., :3] for p in anchors)
        tile_mean, tile_p95 = tile_stats(a, b)
        digest = hashlib.sha256(anchors[0].read_bytes()).hexdigest()[:16]

        for row in pending_rows:
            base_rows.append(row + [pos, fwd, f"{tile_mean:.5f}", f"{tile_p95:.5f}", digest])
        pending_rows.clear()
        if scene in WAVE1_GAME_SCENES and dump.exists():
            material_rows += material_report(dump, scene)

    revision = subprocess.run(["git", "-C", str(REPO), "rev-parse", "--short=12", "HEAD"],
                              capture_output=True, text=True).stdout.strip()
    with open(out / "baselines.tsv", "w") as f:
        f.write("scene\tconfig\tsettled_tier\ttier_oscillation\twarmup_frames\twindow_frames\trt_tier_hist\t"
                "pinned_wall_ms_median\tpinned_wall_ms_min\tpinned_wall_ms_max\tgpu_ms_median\tentities\t"
                "render_extent\tanchor_camera_pos\tanchor_camera_forward\tanchor_tile_floor_mean\t"
                "anchor_tile_floor_p95\tanchor_s0_sha256_16\n")
        for row in base_rows:
            f.write("\t".join(row) + "\n")
    with open(out / "materials.tsv", "w") as f:
        f.write("scene\troute\tclass\tinstances\troughness_p05\troughness_median\troughness_p95\t"
                "metalness_median\tF1\tF2\tF3\n")
        for row in material_rows:
            f.write("\t".join(row) + "\n")
    wall = time.monotonic() - t0
    with open(out / "run-metadata.txt", "w") as f:
        f.write(f"revision={revision}\nruns={args.runs}\nframes={args.frames}\nanchor_frames={args.anchor_frames}\n"
                f"conditions=renderer-stepped orbit, configs taa (native) and fsr-quality, release build, fresh settings per run; anchors at native TAA, no jitter, the TAA settled tier\n"
                f"tile={TILE}\nF1_roughness={F1_ROUGHNESS}\nF3_tolerance={F3_TOLERANCE}\nwall_seconds={wall:.1f}\n")
    log(f"done in {wall:.1f} s: {out / 'baselines.tsv'}, {out / 'materials.tsv'}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
