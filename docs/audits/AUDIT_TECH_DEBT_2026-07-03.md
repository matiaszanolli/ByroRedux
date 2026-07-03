# Tech-Debt Audit — 2026-07-03

**Scope**: All 9 dimensions, `--depth deep`. Full codebase (`crates/` 21 crates + `byroredux/`).
**HEAD**: `8498e559`. **Method**: Discovery recipes per
`.claude/commands/audit-tech-debt/SKILL.md`; every finding re-verified against
live source. Dedup baseline = `gh issue list` (all-state, 200-issue window +
`tech-debt`-labelled 500-issue window) + prior `AUDIT_TECH_DEBT_*.md` reports
(last: 2026-07-02, captured at commit `c739d00e`).

## Executive Summary

The codebase remains in **very clean shape**. Only 29 commits landed between
the 2026-07-02 sweep (`c739d00e`) and this one (`HEAD`) — all targeted bug
fixes with well-documented regression tests (D6-01/D6-02 pose-hash rollback,
#1845 SaveRegistry form-id flag, #1828/#1829 BSGeometry sentinel-slot fix, PEX
BE/Starfield round-trip tests, VWD flag, ragdoll bone-miss logging). None of
that work introduced markers, stubs, or doc rot.

- **Path gate GREEN** — `_audit-validate.sh` reports 1006/1006 refs valid
  across 26 skill files (grew from 980 on 07-02 as new audit reports were added).
- **Zero genuine markers** — TODO/FIXME/HACK/XXX count unchanged at 17, all
  false positives (protocol `XXXX` tag, reference-impl FIXME documentation).
  `unimplemented!/todo!()` still 0.
- **`allow(dead_code)` unchanged at 20** — the one known-redundant annotation
  (`Dx10Chunk::start_mip`, `crates/bsa/src/ba2.rs:148`) is still present;
  already tracked as **#1761** (OPEN), not re-reported here.
- **GPU-struct doc/code sizes still consistent** — `GpuInstance` 112 B,
  `GpuCamera` 336 B, `GpuMaterial` 300 B across `renderer.md`,
  `shader-pipeline.md`, `bindings.glsl`, and the pinning test
  (`gpu_instance_layout_tests.rs`).
- **`Material::classify_pbr` doc comments** still correctly framed as
  deleted/historical; surviving symbols (`classify_pbr_keyword`,
  `Material::resolve_pbr`) unchanged.
- **New**: `crates/core/src/ecs/resources.rs` crossed the 2000-LOC Dim-1
  threshold for the first time (1867 → 2077 LOC) via the #1791/#1796
  pose-hash-rollback fix landing on 2026-07-02 *after* that day's audit
  snapshot was taken — genuinely new, not previously reported.

Findings this pass: **1 total** — 0 CRITICAL, 0 HIGH, 0 MEDIUM, 1 LOW.

| Severity | Count |
|----------|-------|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 0 |
| LOW | 1 |

### Delta vs baseline (2026-07-02, commit `c739d00e`)

| Metric | 07-02 | 07-03 | Note |
|--------|-------|-------|------|
| Marker total | 17 | 17 | unchanged, all false positives |
| `allow(dead_code)` | 20 | 20 | unchanged; TD8-004/#1761 still open, not re-reported |
| `unimplemented!/todo!()` | 0 | 0 | unchanged |
| `#[ignore]` tests (`*.rs` only) | 134 | 134 | unchanged — no new ignore-gated tests landed |
| files >2000 LOC | 6 | 7 | **+1**: `crates/core/src/ecs/resources.rs` (1867→2077) crossed threshold |
| path gate | GREEN (980 refs) | GREEN (1006 refs) | grew as new audit reports were added; still 100% valid |

## Baseline Snapshot (for next audit to diff)

```
TODO/FIXME/HACK/XXX:    17   (all false positives — protocol / ref-impl docs)
allow(dead_code):       20   (19 justified, 1 stale — tracked as #1761/TD8-004)
unimplemented!/todo!(): 0
#[ignore] tests (repo-wide, incl. docs/prose mentions): 277
#[ignore] tests (*.rs only, code-accurate count):        134
files >2000 LOC:        7
```

Oversized set (live, today):
```
4411  crates/renderer/src/vulkan/context/draw.rs        (Existing: #1857 / TD1-001)
3348  crates/renderer/src/vulkan/context/mod.rs          (Existing: #1749 / TD1-004, "new()" 1025-LOC ctor)
2884  byroredux/src/main.rs                              (Existing: #1858 / TD1-003)
2370  crates/nif/src/import/collision.rs                 (pre-existing, no open split issue found)
2166  crates/nif/src/blocks/particle.rs                  (pre-existing, no open split issue found)
2077  crates/core/src/ecs/resources.rs                   (NEW — TD1-2026-07-03-01, this report)
2065  crates/plugin/src/esm/records/actor.rs              (pre-existing, no open split issue found)
```

Note: `collision.rs`, `particle.rs`, and `actor.rs` are carried over from the
07-02 baseline unchanged (no open GitHub issue tracks their split — the SKILL
names them as split-candidate examples, but no prior audit filed a concrete
finding for them; left as pre-existing baseline entries, not re-litigated here
since their LOC did not move this pass).

## Top Quick Wins

None trivial this pass beyond the already-open #1761 (`start_mip` redundant
annotation) — not re-reported, still awaiting a fix commit.

## Top Medium Investments

1. **Split `crates/core/src/ecs/resources.rs` (2077 LOC)** — extract
   `SkinSlotPool` (struct + impl + `Resource` impl + its 456-line test module,
   ~1057 of the file's 2077 lines) into its own submodule; group the
   remaining resources by domain (see TD1-2026-07-03-01 below).
2. Carried over from 07-02 (still open, unchanged): split `context/draw.rs`
   (#1857), `context/mod.rs::new()` (#1749), `byroredux/src/main.rs` (#1858).

---

## Findings

### TD1-2026-07-03-01: `crates/core/src/ecs/resources.rs` crossed 2000 LOC (now 2077)
- **Severity**: LOW
- **Dimension**: 1 (Complexity)
- **Location**: `crates/core/src/ecs/resources.rs` (whole file, 2077 lines)
- **Status**: NEW
- **Description**: The file grew from 1867 → 2077 lines via commits
  `af6e4c9b` (Fix #1791) and `e040231a` (Fix #1796), both landing on
  2026-07-02 — the pose-hash rollback feature for `SkinSlotPool`. Those
  commits post-date the commit (`c739d00e`) the 2026-07-02 tech-debt audit
  captured, so this crossing was invisible to that pass; it is genuinely new
  today. `resources.rs` is explicitly named as a Dim-1 split-candidate in the
  SKILL ("split per resource domain: rendering/world/audio/scripting"), and
  is one of the newer/less-audited resource files per `_audit-common.md`'s
  crate roster.
- **Evidence**: Structural scan of the file (`grep -n '^pub struct\|^impl '`)
  shows four natural domains, none currently separated:
  - L1–228: `SystemList`, `SchedulerAccessReport` (debug/introspection),
    `ScreenshotBridge` (rendering/screenshot), `DeltaTime`/`TotalTime`/
    `EngineConfig` (core loop) — ~228 LOC.
  - L229–479: `DebugStats`, `ScratchRow`, `ScratchTelemetry` (debug/scratch
    telemetry) — ~250 LOC.
  - L480–1537: `SkinCoverageStats`, `CpuFrameTimings`, and **`SkinSlotPool`**
    (struct @690, impl @747–1056, `Resource` impl @1056, then a 456-line
    `#[cfg(test)] mod skin_slot_pool_tests` @1059–1513, plus a trailing
    `impl SkinCoverageStats` @1514) — this single segment is **~1057 lines,
    over half the file**, and is entirely rendering/skinning-domain.
  - L1538–2077: `SelectedRef` (world/editor), `ItemInstance` +
    `ItemInstancePool` (world/inventory) + their test module @1646 — ~539 LOC.
- **Impact**: Every unrelated resource-type edit (e.g. adding a new
  `DebugStats` field) now recompiles and re-reviews against the same file as
  the 1057-line skinning-telemetry block. Not a correctness bug — pure
  edit/review-cost debt, consistent with the LOW default for Dim 1 (no
  amplification trigger from the severity table applies).
- **Related**: Distinct from the already-open #1857/#1749/#1858 (renderer
  `context/` + `main.rs` splits) — this is the first time `resources.rs`
  itself has crossed threshold; no prior issue tracks it.
- **Suggested Fix**: Extract `SkinSlotPool` (struct, impl, `Resource` impl,
  and its test module) into `crates/core/src/ecs/resources/skin_slot_pool.rs`
  — it is both the largest single unit and the most cohesive (one resource,
  self-contained skinning-dispatch-gating logic + regression tests already
  isolated in their own `mod`). Optionally follow with a second pass grouping
  the remaining resources into `resources/{core,debug,world}.rs` per the
  domain boundaries above (`core.rs`: DeltaTime/TotalTime/EngineConfig;
  `debug.rs`: SystemList/SchedulerAccessReport/DebugStats/ScratchRow/
  ScratchTelemetry/ScreenshotBridge/CpuFrameTimings/SkinCoverageStats;
  `world.rs`: SelectedRef/ItemInstance/ItemInstancePool), with `resources.rs`
  becoming a thin `pub use` re-export hub — mirroring the `mod.rs`-as-thin-
  dispatch pattern already used for `cell_loader.rs` and `scene.rs`. Effort:
  medium (mechanical move + re-export wiring; no behavior change).

---

## Verified-Clean (no finding — recorded so the next audit does not re-litigate)

- **Dim 1 (Complexity, everything else)**: `context/draw.rs` (4411, was 4265),
  `context/mod.rs` (3348, was 3335), `main.rs` (2884, was 2846) all grew
  slightly from incremental fixes but are already tracked (#1857, #1749,
  #1858) — not re-reported. `collision.rs` (2370), `particle.rs` (2166),
  `actor.rs` (2065) unchanged from 07-02, no open split issue exists for any
  of the three; left as baseline carry-over, not a new finding.
- **Dim 2 (Duplication)**: All Z-up→Y-up flips still route through the
  canonical `crates/core/src/math/coord.rs` / `crates/nif/src/import/coord.rs`
  / `crates/nif/src/anim/coord.rs` helpers — 19 call sites checked, all use
  the shared helpers, no leaked re-implementation. The new #1845 SaveRegistry
  fix (`is_form_id` flag replacing the `apply.is_none()` heuristic) is itself
  a duplication *fix*, not new debt — it consolidates a previously-implicit
  invariant into an explicit, single-source-of-truth field with a regression
  test (`form_id_column_resolves_the_flagged_entry`).
- **Dim 3 (Doc rot)**: Path gate GREEN (1006/1006 refs, up from 980 as new
  audit reports were added — no STALE hits). GPU-struct byte sizes still
  consistent everywhere checked (112/336/300). `feature-matrix.md` Scripting
  (M47) rows still correctly read "shipped" for M47.0/M47.1/13-function CTDA
  and the M47.2 `.pex` recognizer slice. `Material::classify_pbr` doc
  comments in `material.rs` all still correctly frame it as
  deleted/historical.
- **Dim 4 (Audit rot)**: Gate clean across all 26 skill files; no symbol-anchor
  drift spot-checked in the touched files.
- **Dim 5 (Markers)**: 0 genuine, count unchanged at 17 (same false-positive
  set as 07-02: `XXXX` ESM protocol tag, ref-impl FIXME docs in `bgem.rs`/
  `bs_geometry.rs`, and a resolved-context TODO mention in `scene.rs:791`).
  MIT attribution block atop `triangle.frag` untouched by the shader-constant
  changes in this window.
- **Dim 6 (Stubs)**: `unimplemented!/todo!()` still 0. All "stub"/"placeholder"
  hits are the same previously-verified intentional-fallback set (NIF opaque-
  block skip, SpeedTree billboard, ESM stub-shape captures, `condition.rs`,
  `material/mod.rs` placeholders) — none reachable-and-unfinished. The new
  #1828/#1829 BSGeometry sentinel-slot fix (`bs_geometry.rs`) is a bug fix
  with a 299-line dedicated regression-test sibling
  (`bs_geometry_sentinel_slot_tests.rs`), not a new stub.
- **Dim 7 (Magic numbers)**: GPU sizes still pinned by
  `gpu_instance_layout_tests.rs` (112/336). The touched shader-constant files
  in this window (`shader_constants.rs`, `shader_constants_data.rs`,
  `build.rs`, `shaders/include/shader_constants.glsl`) all route through the
  existing generated-header pipeline — no literal bypasses the
  `shader_constants_data.rs` source of truth.
- **Dim 8 (Dead code)**: Count unchanged at 20 `#[allow(dead_code)]`
  annotations; the one known-stale one (`Dx10Chunk::start_mip`,
  `ba2.rs:148`) is unchanged and already tracked as **#1761 (OPEN,
  TD8-004)** — verified the issue body still matches current code exactly
  (lines 144-151, same evidence). Not re-reported. No new dead code, no new
  `_unused`/`#[deprecated]`/"// removed:" breadcrumbs found in the 71 files
  touched since `c739d00e`.
- **Dim 9 (Test hygiene)**: `#[ignore]` count over `*.rs` unchanged at 134
  (git-diff confirms zero `+#[ignore]` / `-#[ignore]` lines between
  `c739d00e` and `HEAD`). The repo-wide grep total of 277 (vs. 267 quoted in
  the 07-02 report) is prose/markdown drift from new `docs/audits/*.md` and
  `.claude/issues/*/ISSUE.md` files mentioning "#[ignore]" in text, not new
  ignored tests — confirmed via commit-pinned `git grep -F -c` comparison
  showing 134 on both `c739d00e` and `HEAD`. New tests added this window
  (DA10 `.pex` byte-equality parity, Skyrim-BE/Starfield-guards PEX
  round-trip, sentinel-slot regression, requeue/rollback pose-hash tests)
  all carry field-level assertions, not smoke-only `assert!(result.is_ok())`.

## Deferred

None. No finding is gated on an in-progress milestone.
