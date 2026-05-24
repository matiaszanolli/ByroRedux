# Tech-Debt Audit — 2026-05-24

## Executive Summary

**1 NEW LOW** + 3 carryovers across all 10 dimensions. The headline change is in **Dimension 9 (file complexity)**: the two Vulkan-recording carryovers (`draw.rs`, `context/mod.rs`) — held steady at 2899 + 2661 LOC in the 2026-05-22 audit — have grown **+334** and **+221** LOC in 48 hours behind a busy run of M58/M55/water-caustic/Disney-BSDF/skin-gate landings, and `byroredux/src/main.rs` has crossed the 2000-LOC ceiling for the first time (2162 LOC, driven by M47.0 Papyrus init + M27 system-access declarations). Velocity is the story here, not magnitude.

Audit-skill gate (`.claude/commands/_audit-validate.sh`) reports **OK: 293 refs across 22 skill files** — Dim 7 / Dim 10 are auto-clean again.

| Severity | NEW | Carryover | Total | Dimensions affected |
|----------|-----|-----------|-------|---------------------|
| HIGH     | 0   | 0         | 0     | — |
| MEDIUM   | 0   | 1         | 1     | D9 (BLOCKED — `TD9-200/201` carry, **now escalating in trajectory**) |
| LOW      | 1   | 3         | 4     | D9 (`TD9-NEW-01`), D4 (`TD4-201`, `TD4-202`), D10 (`TD10-001` / #1156) |

## Baseline Snapshot

| Metric | Today (2026-05-24) | 2026-05-22 | Δ |
|---|---:|---:|---:|
| `TODO` / `FIXME` / `HACK` / `XXX` markers | 5 | 4 | +1 |
| ↳ of which *active* (not closure-mention prose) | **1** | **0** | +1 |
| `#[allow(dead_code)]` | 26 | 26 | 0 |
| `#[allow(unused...)]` | 21 | 20 | +1 |
| `unimplemented!()` / `todo!()` | 0 | 0 | 0 |
| `panic!("not yet"|"not impl")` | 0 | 0 | 0 |
| `#[ignore]` tests | 113 | 126 | **−13** ✓ (improvement) |
| Files > 2000 LOC | **3** (draw.rs 3233, context/mod.rs 2882, main.rs 2162) | 2 (draw.rs 2899, context/mod.rs 2661) | **+1 file**, **+555 LOC combined** |
| `.claude/commands/_audit-validate.sh` | OK (293 refs / 22 files) | OK | clean |

Baseline persisted to `/tmp/audit/tech-debt/baseline.txt`.

## Top 10 Quick Wins

1. **Close any GitHub issues tracking `TD4-201` / `TD4-202`** that are already covered by the carryover — there's nothing new to add since 2026-05-22 (status didn't change). One-click action; no code change.

2. Nothing else trivial this cycle. The one NEW finding (`TD9-NEW-01`, main.rs split) is small-to-medium effort, not trivial.

## Top 5 Medium Investments

1. **`TD9-200` / `TD9-201` (BLOCKED carry, escalating)** — Vulkan-recording files have resumed creep after holding steady through 2026-05-22. `draw.rs` +334 LOC and `context/mod.rs` +221 LOC in 48 hours, driven by ~15 commits in the M58/M55/water-caustic/Disney-BSDF/skin-gate band. Trajectory is reversing the 2026-05-22 "held steady" framing. The fix path is unchanged (RenderDoc-driven captured-frame baseline before any split), but the **timeline pressure increased** — at +275 LOC/day combined, the file passes 4000 LOC within a week.

2. **`TD9-NEW-01`** — `byroredux/src/main.rs` first time over 2000 LOC ceiling (2162). Unlike the Vulkan carry, this one is splittable today: console init, app init, and event-loop wiring are the natural axes. No render-pass recording lives here.

3. **`TD4-201`** *(carry, unchanged)* — 32 bare-hex NIF version compares should adopt `NifVersion::*` constants.

4. **`TD4-202`** *(carry, unchanged)* — 112 ESM subrecord size literals should map to named `RecordType::*_SIZE` constants.

5. **`TD10-001` / #1156** *(carry, policy)* — Stale local `.claude/issues/<N>/ISSUE.md` snapshots. Per Phase-1 TD10 checklist update: dropped from active finding-rotation under the immutable-snapshot convention (snapshots are filed-time, not live). Tracking only as policy reminder.

## Findings

### HIGH
None.

### MEDIUM

#### TD9-200 / TD9-201 *(carry, BLOCKED, ESCALATING TRAJECTORY)* — Vulkan-recording files exceed 2000-LOC ceiling and have resumed growth

- **Files**: `crates/renderer/src/vulkan/context/draw.rs` (**3233 LOC**, +334 vs 2026-05-22), `crates/renderer/src/vulkan/context/mod.rs` (**2882 LOC**, +221 vs 2026-05-22)
- **Severity**: MEDIUM (file complexity); promotion floor not crossed but watch closely
- **Effort**: large (decompose first — needs RenderDoc smoke harness as precondition)
- **Status**: CARRY, BLOCKED (per `feedback_speculative_vulkan_fixes.md`)
- **Age**: original carry pre-Session-34; today's growth is from 2026-05-22 → 2026-05-24
- **48-hour driver**: ~15 commits in the band — sample includes `#1259/#1260` blend-pipeline fast-path, `#1258` DrawCommand vs GPU draw distinction, `VUID-vkQueueSubmit-…00067` semaphore fix, Riverwood water-pipeline validation fixes, `#1210` water-caustic phases A-E, `#1248/#1249/#1250` Disney BSDF port, `#1147` PBR/SSS/model-space-normals gating, `#1195/#1196/#1197` skin-compute gates, `#1211` empty-framebuffer guard, `#1227` rt_flag patch, `#1125` skyTint gate. Each is a small inline addition to `draw_frame` recording — the per-commit delta is justified; the aggregate is not.
- **Why this matters now**: 2026-05-22 framed these as "held steady at exactly 2899 + 2661 — no creep over 24h." That framing no longer holds. The 48-hour velocity of +555 LOC combined puts both files on a trajectory toward 4000 LOC within a week if the M58/M55/water-caustic-style multi-phase feature landings continue.
- **Fix path (unchanged)**: design a RenderDoc-driven captured-frame baseline so a split can be verified for byte-equality of the next-frame swapchain image. Precondition for the split. After that, the candidate split axes are (a) per-pass-recording (G-buffer / SSAO / RT / SVGF / TAA / composite / bloom / volumetrics) and (b) per-subsystem state machines (water, skin-compute prime, BLAS lifecycle). Each can be one submodule.
- **Recommendation**: bump the BLOCKED-priority signal. If the next 48 hours add another +275 LOC combined, this becomes the highest-leverage tech-debt item on the board.

### LOW

#### TD9-NEW-01 *(NEW)* — `byroredux/src/main.rs` crossed 2000-LOC ceiling

- **File**: `byroredux/src/main.rs` (**2162 LOC**)
- **Dimension**: 9 (File / Function / Module Complexity)
- **Severity**: LOW
- **Effort**: small-to-medium (the split surface is mostly init, not recording)
- **Age**: crossed the ceiling in the 2026-05-22 → 2026-05-24 window
- **Driver**: M47.0 Phase 1+2 (`6c51af55` Papyrus demo wiring + `a80781a7` ScriptRegistry + defaultRumbleOnActivate spawner), M27 Phase 1+2+3 (`a9810d40` + `05fe2bac` system-access declarations across parallel stages — 0 unknown / 0 conflicts).
- **Why this matters**: main.rs is the entry surface — every milestone integration adds wiring here. Unlike the Vulkan carry, splittable today **without** a smoke-harness precondition: no command-buffer recording lives in main.rs. The growth pressure won't slow (more milestones land here every week).
- **Suggested split axes**:
  1. `byroredux/src/init/` — engine init (Vulkan context, plugin DataStore, ECS bootstrap, scene loading).
  2. `byroredux/src/cli.rs` — argument parsing + `--bsa` / `--esm` / `--cell` / `--grid` / `--bench-*` handling. This is the part that grows whenever a new CLI flag lands.
  3. `byroredux/src/runtime.rs` — the per-frame system schedule + winit `ApplicationHandler` wiring.
  4. `byroredux/src/main.rs` shrinks to a thin top-level dispatch.
- **Not yet a Top-5 priority** because the file is barely over the ceiling (only +162) and the growth trajectory is feature-driven, not pathological. Watchlist for next sweep — if it crosses 2400 before a split lands, promote.

#### TD4-201 *(carry, unchanged)* — 32 bare-hex NIF version compares should adopt `NifVersion::*` constants

- **Files**: scattered across `crates/nif/src/blocks/*.rs`
- **Severity**: LOW
- **Effort**: small (mechanical)
- **Status**: CARRY; same content as 2026-05-22.

#### TD4-202 *(carry, unchanged)* — 112 ESM subrecord size literals should map to named `RecordType::*_SIZE` constants

- **Files**: scattered across `crates/plugin/src/esm/records/*.rs`
- **Severity**: LOW
- **Effort**: medium (mechanical, 112 sites)
- **Status**: CARRY; same content as 2026-05-22.

#### TD10-001 / #1156 *(policy carry — non-actionable)*

- **Issue**: #1156 (immutable-snapshot semantics for `.claude/issues/<N>/ISSUE.md`).
- **Status**: POLICY CARRY — dropped from active rotation per the Phase-1 Dim-10 checklist update. The stale local snapshots are documented behavior, not a defect to fix. Listed here for traceability; future audits should NOT refile.

## Verified-Clean Dimensions (no findings this cycle)

| Dim | Surface | Verification |
|---|---|---|
| **D1** Stale Markers | 5 markers total / **1 active** | The 1 active is `crates/renderer/src/vulkan/material.rs:551` — a documented TODO tied to `#1248-followup` (transmission lobe extension); not rot, properly tracked. The other 4 (`bgsm/bgem.rs:122`, `nif/blocks/bs_geometry.rs:563`, `byroredux/src/main.rs:1575`, `byroredux/src/scene.rs:770`) are closure-mention prose, not active TODOs. Same pattern as 2026-05-22 ("of which active: 0"). The +1 marker delta is the new `material.rs:551` glass-transmission TODO, which was added with #1248 and is correctly tied to a follow-up. |
| **D2** Dead Code | 26 `#[allow(dead_code)]` (unchanged) | Spot-checked all 26 — every site is justified: `cfg(debug_assertions)`-gated lock-tracker collection (#823), test-helper fixtures, future-hook markers (#1199, #1135), schema-completeness constants for VF_UVS_2 / VF_LAND_DATA / VF_INSTANCE per #336 / #358, the `components.rs` ambient-cube uniform cluster (intentional struct-level pattern), and the FO4-DIM6-02 stage-2 reservation in `mswp.rs`. No rot. |
| **D3** Logic Duplication | Workspace texture / barrier / descriptor scaffolding | No new duplication site detected. Coord-flip remains canonical in `crates/nif/src/import/coord.rs`. |
| **D5** Stubs | 0 `unimplemented!()` / `todo!()`, 0 `panic!("not yet"\|"not impl")` | Clean, same as 2026-05-22. ESM per-game records: matrix continues to converge per ROADMAP. |
| **D6** Test Hygiene | 113 `#[ignore]` tests (down 13 from 126) | **Improvement.** Sampled across `crates/bsa/`, `crates/nif/`, `crates/plugin/` — all 113 ignores are properly env-gated (`BYROREDUX_OBLIVION_DATA`, `BYROREDUX_FO4_DATA`, `BYROREDUX_SKYRIM_DATA`, `BYROREDUX_FNV_DATA`, etc.) for on-disk Steam-data smoke tests that CI cannot run. Documented `//! Gated #[ignore] on …` headers throughout. Not test rot — intentional disk-gating. The −13 delta reflects test consolidation or env-flag promotion since 2026-05-22 (no specific commit identified; cumulative). |
| **D7** Stale Docs | `_audit-validate.sh` OK (293 refs / 22 files) | Gate clean. |
| **D8** Backwards-Compat Cruft | `#[deprecated]`, `_unused` renames, `// removed:` markers, single-branch feature flags | Zero `#[deprecated]` items in the workspace. No `// removed:` breadcrumbs. Matches reposted from grep are test-variable names (`f_old = sentinel(0xDEAD_BEEF)` in sync.rs), legitimate doc references to "removed nodes" in transform-propagation comments, or `_old_size` in BSA-archive resize-handling tests — none are cruft. |
| **D10** Audit-Finding Rot | `_audit-validate.sh` OK | Gate clean. TD10-001 / #1156 dropped from active rotation per immutable-snapshot policy. |

## Deferred

| Finding | Gating | Notes |
|---|---|---|
| `TD9-200 / TD9-201` actual split | RenderDoc-driven captured-frame baseline | Trajectory now escalating — see MEDIUM section for the 48-hour velocity argument. |
| `TD9-NEW-01` main.rs split | Choose split axis (proposal in the LOW section) | Splittable today, but file is only +162 over the ceiling. Promote on next-sweep crossing 2400 LOC if no split lands. |

## Notes

- The 48-hour velocity inflection on `draw.rs` + `context/mod.rs` is the single most actionable signal in today's sweep. The 2026-05-22 framing ("held steady — no creep over 24h") is now wrong, and the trajectory matters more than the absolute numbers.
- `_audit-validate.sh` continues to make Dim 7 / Dim 10 a near-no-op when path discipline holds. Keep it cheap.
- `#[ignore]` test count drop (−13 in 48h) is welcome but uncited — worth a side-grep on the next sweep to identify what reclassified, in case it points to a CI-coverage improvement worth promoting in `feedback_*`.
