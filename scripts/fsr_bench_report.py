#!/usr/bin/env python3
"""Summarize the FSR benchmark matrix produced by fsr-bench-matrix.sh.

Reports two quantities the plan is explicit about not conflating:

  render-work recovered = native(render-resolution passes)
                        - preset(render-resolution passes)
  net frame recovery    = native end-to-end frame time
                        - preset end-to-end frame time

The first is the gross saving from shading fewer pixels. The second is what a
player actually gets, after paying for the upscale dispatch and for the
output-resolution work no preset shrinks. Quoting only the first would
overstate the win, which is precisely the substitution
docs/engine/fsr3-upscaler-integration-plan.md §1.7 rules out.

Per-run values are summarized as median with full range, matching the
bench-of-record convention. Median rather than mean because a single
scheduler hiccup on a shared desktop skews a 3-run mean badly, and the
existing ROADMAP numbers were gathered the same way.
"""

import sys
from collections import defaultdict

# Passes that run at render resolution and therefore shrink with the preset.
# `presentation` and `upscale` are deliberately excluded: the first runs at
# output resolution, the second is the cost of the preset itself.
RENDER_RES_PASSES = [
    "gpu_main",
    "gpu_svgf",
    "gpu_composite",
    "gpu_ssao",
    "gpu_volumetrics",
    "gpu_bloom",
]

REFERENCE = "taa"

# Preset display order — ascending upscale ratio, so a reader scans the table
# top-to-bottom as "more upscaling".
ORDER = ["taa", "fsr-native-aa", "fsr-quality", "fsr-balanced", "fsr-performance"]

# Columns that identify the measured scene state. `f19f7f15` added them to the
# harness together with the switch from a parked camera to
# `--bench-mode renderer-stepped`; every TSV archived before that commit has a
# 17-column header without them (#2835).
FINGERPRINT_COLUMNS = [
    "mode",
    "camera",
    "sim_time_s",
    "entities",
    "draws",
    "lights",
    "tlas",
    "state_hash",
]

# Non-numeric columns — kept as strings rather than coerced to floats.
TEXT_COLUMNS = ("scene", "config", "run", "mode", "camera", "draws", "state_hash")

MISSING = "-"


def median(values):
    values = sorted(values)
    n = len(values)
    if n == 0:
        return 0.0
    mid = n // 2
    return values[mid] if n % 2 else (values[mid - 1] + values[mid]) / 2


def main(path):
    with open(path) as handle:
        # `#`-prefixed provenance lines (harness commit, capture date) are
        # metadata, not data — skip them wherever they appear.
        lines = [line for line in handle if line.strip() and not line.startswith("#")]

    if not lines:
        print("no rows — every run failed to produce a bench line")
        return 1

    header = lines[0].rstrip("\n").split("\t")
    rows = [dict(zip(header, line.rstrip("\n").split("\t"))) for line in lines[1:]]

    if not rows:
        print("no rows — every run failed to produce a bench line")
        return 1

    # #2835 — an archived TSV taken before `f19f7f15` has no scene-state
    # columns. Reading one used to raise `KeyError: 'mode'`, i.e. the tool
    # crashed on the very artefacts the repo keeps so cross-commit comparisons
    # stay checkable. Detect the older schema, report the fingerprint gate as
    # UNAVAILABLE rather than silently passing it, and print the timings —
    # which are present and readable in both schemas.
    missing_columns = [c for c in FINGERPRINT_COLUMNS if c not in header]
    if missing_columns:
        print(
            f"note: {path} predates the scene-state fingerprint schema "
            f"(f19f7f15) — missing {', '.join(missing_columns)}. Timings below "
            "are readable, but the fingerprint gate CANNOT run: these rows are "
            "not verified to describe the same measured scene state, and they "
            "were captured on the pre-f19f7f15 parked-camera workload, so they "
            "are not comparable with a current-harness run."
        )

    # scene -> config -> list of per-run dicts of floats
    grouped = defaultdict(lambda: defaultdict(list))
    for row in rows:
        parsed = {}
        for key, value in row.items():
            if key in TEXT_COLUMNS:
                parsed[key] = value
            else:
                try:
                    parsed[key] = float(value)
                except ValueError:
                    parsed[key] = 0.0
        grouped[row["scene"]][row["config"]].append(parsed)

    invalid_state = False
    for scene in grouped:
        configs = grouped[scene]
        if REFERENCE not in configs:
            print(f"\n=== {scene}: no {REFERENCE} reference captured, skipping\n")
            continue

        # `.get` throughout: a pass column absent from an older schema reads as
        # a zero contribution rather than taking the whole report down.
        def stat(config, key):
            return median([r.get(key, 0.0) for r in configs[config]])

        def spread(config, key):
            values = [r.get(key, 0.0) for r in configs[config]]
            return min(values), max(values)

        ref_frame = stat(REFERENCE, "wall_ms")
        ref_render = sum(stat(REFERENCE, p) for p in RENDER_RES_PASSES)
        entities = int(stat(REFERENCE, "entities"))
        runs = len(configs[REFERENCE])

        # Every row in an upscaler comparison must describe the same measured
        # scene state. A mismatch invalidates the matrix before any frame-time
        # delta is interpreted, even when the aggregate counts happen to agree.
        all_rows = [row for config_rows in configs.values() for row in config_rows]
        fingerprints = {
            tuple(row.get(column, MISSING) for column in FINGERPRINT_COLUMNS)
            for row in all_rows
        }
        # Only a schema that actually carries the columns can fail the gate.
        # On an older TSV every row yields the same all-`MISSING` tuple, which
        # would otherwise read as a clean pass — the header note above is what
        # keeps that from being mistaken for verification.
        if not missing_columns and len(fingerprints) != 1:
            invalid_state = True
            print(f"\n=== {scene}: INVALID — {len(fingerprints)} scene-state fingerprints")
            for fingerprint in sorted(fingerprints, key=str):
                print("  " + " | ".join(map(str, fingerprint)))
            continue
        mode, camera, *_ = next(iter(fingerprints))
        if missing_columns:
            mode, camera = "unverified", "parked (pre-f19f7f15)"

        print(
            f"\n=== {scene} — {mode}/{camera}, {entities} entities, "
            f"{runs} runs, median (min–max)"
        )
        print(
            f"{'config':<17}{'fps':>8}{'frame ms':>14}{'render ms':>11}"
            f"{'upscale':>9}{'present':>9}{'render rec.':>16}{'net rec.':>16}"
        )
        for config in ORDER:
            if config not in configs:
                continue
            fps = stat(config, "wall_fps")
            frame = stat(config, "wall_ms")
            render = sum(stat(config, p) for p in RENDER_RES_PASSES)
            upscale = stat(config, "gpu_upscale")
            present = stat(config, "gpu_presentation")
            lo, hi = spread(config, "wall_ms")

            if config == REFERENCE:
                render_rec = net_rec = ""
            else:
                render_delta = ref_render - render
                net_delta = ref_frame - frame
                render_rec = (
                    f"{render_delta:+.2f} ({render_delta / ref_render * 100:+.0f}%)"
                    if ref_render > 0
                    else f"{render_delta:+.2f}"
                )
                net_rec = (
                    f"{net_delta:+.2f} ({net_delta / ref_frame * 100:+.0f}%)"
                    if ref_frame > 0
                    else f"{net_delta:+.2f}"
                )

            frame_cell = f"{frame:.2f} ±{(hi - lo) / 2:.2f}"
            print(
                f"{config:<17}{fps:>8.1f}{frame_cell:>14}"
                f"{render:>11.2f}{upscale:>9.3f}{present:>9.3f}"
                f"{render_rec:>16}{net_rec:>16}"
            )

        print(
            "  render ms = "
            + " + ".join(p.replace("gpu_", "") for p in RENDER_RES_PASSES)
            + " (render-resolution passes only; upscale and present excluded)"
        )
    return 1 if invalid_state else 0


def self_test():
    """#2835 regression — a pre-`f19f7f15` 17-column TSV must be readable.

    The archived tables are kept precisely so cross-commit FSR comparisons stay
    checkable, and before this fix the tool that produced them raised
    `KeyError: 'mode'` on its own artefacts. Exercised in CI because there is no
    Python test runner in this repo and the archives are only read by hand.
    """
    import io
    import os
    import tempfile
    from contextlib import redirect_stdout

    legacy_header = (
        "scene\tconfig\trun\twall_fps\twall_ms\tfence_ms\tbrd_ms\tgpu_main\t"
        "gpu_svgf\tgpu_composite\tgpu_ssao\tgpu_volumetrics\tgpu_upscale\t"
        "gpu_presentation\tgpu_bloom\tentities\tdraws"
    )

    def legacy_row(config, wall_ms, main_ms):
        return (
            f"cornell\t{config}\t1\t{1000 / wall_ms:.1f}\t{wall_ms}\t0.1\t0.1\t"
            f"{main_ms}\t0.1\t0.1\t0.1\t0.1\t0.15\t0.01\t0.1\t25\t120"
        )

    cases = {
        "legacy 17-column": "\n".join(
            [
                legacy_header,
                legacy_row("taa", 3.09, 2.0),
                legacy_row("fsr-quality", 2.16, 1.0),
            ]
        )
        + "\n",
        # A current-schema table must still work, and its fingerprint gate must
        # still fire — the tolerance above must not have disabled it.
        "current 23-column": (
            "# harness=deadbeef engine=cafef00d\n"
            "scene\tconfig\trun\tmode\tcamera\twall_fps\twall_ms\tfence_ms\tbrd_ms\t"
            "gpu_main\tgpu_svgf\tgpu_composite\tgpu_ssao\tgpu_volumetrics\t"
            "gpu_upscale\tgpu_presentation\tgpu_bloom\tsim_time_s\tentities\t"
            "draws\tlights\ttlas\tstate_hash\n"
            "cornell\ttaa\t1\trenderer-stepped\torbit\t323.6\t3.09\t0.1\t0.1\t2.0\t"
            "0.1\t0.1\t0.1\t0.1\t0.01\t0.01\t0.1\t5.0\t25\t120\t3\t1\tabc123\n"
            "cornell\tfsr-quality\t1\trenderer-stepped\torbit\t462.1\t2.16\t0.1\t0.1\t"
            "1.0\t0.1\t0.1\t0.1\t0.1\t0.16\t0.01\t0.1\t5.0\t25\t120\t3\t1\tabc123\n"
        ),
    }

    failures = []
    for label, content in cases.items():
        with tempfile.NamedTemporaryFile("w", suffix=".tsv", delete=False) as handle:
            handle.write(content)
            path = handle.name
        try:
            buffer = io.StringIO()
            with redirect_stdout(buffer):
                code = main(path)
            output = buffer.getvalue()
        except Exception as error:  # noqa: BLE001 — any exception is the bug
            failures.append(f"{label}: raised {type(error).__name__}: {error}")
            continue
        finally:
            os.unlink(path)

        if code != 0:
            failures.append(f"{label}: exit {code}, expected 0")
        if "fsr-quality" not in output:
            failures.append(f"{label}: timings missing from report")
        legacy = label.startswith("legacy")
        noted = "predates the scene-state fingerprint schema" in output
        if legacy and not noted:
            failures.append(
                f"{label}: read cleanly but did not warn that the fingerprint "
                "gate could not run — a silent pass is the wrong fix"
            )
        if not legacy and noted:
            failures.append(f"{label}: current schema wrongly flagged as legacy")

    for failure in failures:
        print(f"FAIL {failure}")
    if failures:
        return 1
    print(f"ok — fsr_bench_report self-test passed ({len(cases)} schemas)")
    return 0


if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1] == "--self-test":
        sys.exit(self_test())
    sys.exit(main(sys.argv[1] if len(sys.argv) > 1 else "target/fsr-bench/raw.tsv"))
