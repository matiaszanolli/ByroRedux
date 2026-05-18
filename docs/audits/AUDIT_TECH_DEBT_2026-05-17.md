# Tech-Debt Audit — 2026-05-17

**Scope**: 10 dimensions, deep depth.
**Prior baselines**: `AUDIT_TECH_DEBT_2026-05-13.md`, `AUDIT_TECH_DEBT_2026-05-14.md`, `AUDIT_TECH_DEBT_2026-05-16.md`.

---

## Executive Summary

**10 NEW findings + 14 carryovers from 2026-05-16** across 10 dimensions.

| Severity | New | Carryover | Total | Dimension Distribution (new) |
|----------|-----|-----------|-------|------------------------------|
| MEDIUM   | 1   | 4         | 5     | D10 (1) — systemic local-issue-state drift |
| LOW      | 9   | 10        | 19    | D3 (1), D4 (3), D7 (3), D8 (2) |
| INFO/PASS | —  | —         | —     | D1 / D2 / D5 / D6 / D9 — verified clean or known-deferred |

**Baseline counts (delta vs 2026-05-16)**:
- `TODO/FIXME/HACK/XXX`: 4 → 4 (stable; 2 yesterday's MEDIUMs were already reframed as closed-issue traceback)
- `#[allow(dead_code)]`: 25 → 25 (stable; all 25 verified justified — 9 CellLightingRes staged-rollout markers, 6 NIF schema constants, 2 BA2 mip fields, 8 misc)
- `unimplemented!()` / `todo!()`: 1 → 0 (yesterday's TD5-001 closure landed)
- `#[ignore]` tests: 105 → 105 (stable; all sampled justifications hold — data-gating, harness-gating, regression pinning)
- Files >2000 LOC: 2 → 2 (`draw.rs` 2656, `context/mod.rs` 2487 — both Vulkan-recording, BLOCKED per `feedback_speculative_vulkan_fixes.md`)

**The pattern that won't die (Dim 10, again)**: today the `_audit-validate.sh` gate (#1114) reports clean across 287 path refs in 22 skill files, but the OTHER drift class — `.claude/issues/<N>/ISSUE.md` `State: OPEN` headers vs `gh issue view` `CLOSED` state — has metastasized to **80 stale snapshots**. The structural fix needs a decision: are local issue files canonical or advisory?

**Wins since yesterday**: TD8-016..019 (4 cruft items), TD5-001 (sole `todo!()`), TD2/dead-code baseline holding at 25.

---

## Baseline Snapshot

```
Date: 2026-05-17
TODO/FIXME/HACK/XXX: 4
allow(dead_code): 25
unimplemented!/todo!(): 0
#[ignore] tests: 105
files >2000 LOC: 2
  - crates/renderer/src/vulkan/context/draw.rs (2656)
  - crates/renderer/src/vulkan/context/mod.rs (2487)
```

---

## Top 10 Quick Wins (trivial effort, immediate payoff)

1. **TD4-301 / TD7-052** *(same site)* — Update `crates/renderer/src/vulkan/skin_compute.rs:263` comment: `32768 / 128 = 256` → `32768 / 144 ≈ 227` (stale after #1135).
2. **TD8-024** — Strip the orphaned `build_skinned_blas` reference from the `SKINNED_BLAS_FLAGS` doc comment at `crates/renderer/src/vulkan/acceleration/constants.rs:87` (function deleted in #1141).
3. **TD7-051** — CLAUDE.md:110 claims `Vertex` is "25 floats"; actual is 19 floats + 4 u32 (bone indices) + 8 u8 (splat weights). Size 100 B is correct; the float count is wrong.
4. **TD8-023** — Delete dead re-export alias `TreeObjectBounds` at `crates/plugin/src/esm/records/mod.rs:66` (zero callers; ByroRedux has no external consumers).
5. **TD4-302** — Remove redundant `const uint THREADS_PER_CLUSTER = 32;` at `crates/renderer/shaders/cluster_cull.comp:30` (already `#define`d via `shader_constants.glsl` include — drift risk).
6. **TD4-303** — Replace hardcoded `local_size_x = 8, local_size_y = 8` with `WORKGROUP_X / WORKGROUP_Y` in 4 shaders: `taa.comp:16`, `ssao.comp:9`, `svgf_temporal.comp:21`, `caustic_splat.comp:25`. Bloom + volumetrics already use the constant — these are the outliers.
7. **TD3-207** — Replace inline `ImageSubresourceRange { COLOR, 0, 1, 0, 1 }` literals with `super::descriptors::color_subresource_single_mip()` at `bloom.rs:490` and `caustic.rs:726/768/800` (helper already exists; TAA / swapchain / volumetrics / gbuffer already migrated).
8. **TD2-201..204 *(carryover)*** — 4 trivial-effort dead-code cleanups from yesterday (unused doc comment / unused import / `LightHeader.count` annotation / `VF_FULL_PRECISION` aspirational comment).

Total quick-win effort: **~2.5 hours, ~30 LOC delta** — almost entirely sed-able mechanical edits.

---

## Top 5 Medium Investments (carryover from prior audits)

These remain open from prior audits — not new for today:

1. **TD4-201 (LOW → MEDIUM if adoption stalls)** — 32 bare-hex NIF version compares should adopt `NifVersion::*` constants. Mechanical, but spread across 12+ block files.

2. **TD4-202** — 142 ESM subrecord size literals (`if data.len() == 24`) should map to named `RecordType::DATA_SIZE` constants. Tracked but slow; blocks/* parsers are independent.

3. **TD9-200 / TD9-201 *(BLOCKED MEDIUM)*** — `context/draw.rs` (2656 LOC) and `context/mod.rs` (2487 LOC). Vulkan per-frame recording + device/swapchain init. Splits BLOCKED per `feedback_speculative_vulkan_fixes.md` until RenderDoc baseline / integration test infrastructure lands. Tracked under #1118.

4. **TD10-001 (MEDIUM, NEW)** — 80 stale `.claude/issues/<N>/ISSUE.md` files marking `State: OPEN` while GitHub shows `CLOSED`. Needs a workflow decision before mass-update; see Dim 10.

5. **TD3-202 *(carryover)*** — `EXTERIOR_CELL_UNITS = 4096.0` const + `cell_grid_to_world_yup` helper. Now resolved — Dim 3 today verified the const is in `crates/core/src/math/coord.rs:41` and adoption is wide. **Closing.**

---

## Findings — Grouped by Severity

### MEDIUM (1 new)

#### TD10-001 — 80 stale `.claude/issues/<N>/ISSUE.md` files mark issues as OPEN while GitHub says CLOSED

- **Severity**: MEDIUM (systemic operational-record drift)
- **Dimension**: Audit-Finding Rot
- **Location**: `.claude/issues/<N>/ISSUE.md` (80 files; verified examples: #1076, #1077, #1135)
- **Stale claim**: Local files declare `State: OPEN` / `Status: OPEN`
- **Reality**: `gh issue view <N> --json state` returns `CLOSED` for all 80
- **Root cause**: Local issue files are created/edited during `/fix-issue` flow but never auto-synced when an issue is closed on GitHub
- **Decision pending**: Are local ISSUE.md files canonical or advisory? Options:
  - **A**: Add a post-`gh issue close` sync hook that updates the local `Status` field
  - **B**: Deprecate the local `Status` field and always query GitHub for state
  - **C**: Treat `.claude/issues/` as immutable commit history (snapshot at creation, never updated)
- **Effort**: trivial per file × 80 = small (sed one-liner); medium for workflow change to prevent recurrence
- **Why MEDIUM**: Recurrent operational confusion. The `/fix-issue` skill could spuriously treat closed issues as actionable; future audit agents have already had to spend tokens cross-checking via `gh issue view` per finding.

### LOW (9 new)

#### TD3-207 — Inline `ImageSubresourceRange { COLOR, 0, 1, 0, 1 }` literals in bloom + caustic

- **Dimension**: Logic Duplication
- **Locations**: `crates/renderer/src/vulkan/bloom.rs:490` (1 site reused 2×), `crates/renderer/src/vulkan/caustic.rs:726/768/800` (3 sites)
- **Pattern length**: 6 lines per literal
- **Proposed consolidation**: `super::descriptors::color_subresource_single_mip()` (helper already exists at `descriptors.rs:99-107`; TAA / swapchain / volumetrics / gbuffer already migrated)
- **Effort**: small
- **Why LOW**: Mechanical migration; the canonical helper is in place. Bloom + caustic are the outliers.

#### TD4-301 — Stale `MAX_BONES_PER_MESH` calculation in `skin_compute.rs` doc comment

- **Dimension**: Magic Numbers (documentation drift)
- **Location**: `crates/renderer/src/vulkan/skin_compute.rs:263`
- **Stale text**: `MAX_TOTAL_BONES / MAX_BONES_PER_MESH = 32768 / 128 = 256`
- **Reality**: `MAX_BONES_PER_MESH` was bumped 128 → 144 in today's #1135. The math now reads `32768 / 144 ≈ 227`.
- **Effort**: trivial
- **Cross-listed**: Same site flagged independently by Dim 7 as TD7-052 — deduped here under Dim 4.

#### TD4-302 — Redundant `THREADS_PER_CLUSTER` redefinition in `cluster_cull.comp`

- **Dimension**: Magic Numbers (redundant hardcoding)
- **Location**: `crates/renderer/shaders/cluster_cull.comp:30`
- **Duplicate**: `const uint THREADS_PER_CLUSTER = 32;`
- **Already defined by**: `#include "include/shader_constants.glsl"` (line 28 emits `#define THREADS_PER_CLUSTER 32`)
- **Risk**: Future Rust-side const change won't propagate until shader is manually recompiled AND the duplicate is updated. Lockstep drift hazard per `feedback_shader_struct_sync.md`.
- **Effort**: trivial — delete the manual `const uint` line.

#### TD4-303 — 4 compute shaders hardcode `local_size_x = 8` instead of `WORKGROUP_X`

- **Dimension**: Magic Numbers (duplicated workgroup sizing)
- **Locations**: `taa.comp:16`, `ssao.comp:9`, `svgf_temporal.comp:21`, `caustic_splat.comp:25`
- **Canonical**: `WORKGROUP_X = WORKGROUP_Y = 8` in `shader_constants_data.rs:34-35`, included via `shader_constants.glsl`
- **Contrast**: `bloom_*.comp` + `volumetrics_*.comp` correctly use `layout(local_size_x = WORKGROUP_X, local_size_y = WORKGROUP_Y)` — these 4 are the outliers.
- **Risk**: If WORKGROUP sizes get tuned for occupancy, these 4 shaders silently miss the rebalance.
- **Effort**: small (sed across 4 files; requires SPV regeneration after).

#### TD7-051 — `CLAUDE.md` claims Vertex is "25 floats"; actual is 19

- **Dimension**: Stale Documentation
- **Location**: `CLAUDE.md:110`
- **Stale text**: `Vertex (position + color + normal + uv + bone_idx + bone_wt + splat0/1 + tangent), 9 attribute descriptions, 100 B / 25 floats`
- **Reality**: Vertex is 19 floats + 4 u32 (bone_indices) + 8 u8 (splat_weights). Size 100 B is correct; float count is wrong.
- **Effort**: trivial — update to `100 B = 19 floats + 4 u32 (bone indices) + 8 u8 (splat weights)`.

#### TD7-053 — Audit-archive references to deleted `build_skinned_blas` function

- **Dimension**: Stale Documentation (archival records — informational only)
- **Locations**: `docs/audits/AUDIT_CONCURRENCY_2026-05-16.md:162-174` (6 refs); `AUDIT_SAFETY_2026-05-16.md:55` (1 ref); `AUDIT_PERFORMANCE_2026-05-16.md:266-284` (3 refs); plus 10+ siblings
- **Reality**: Function deleted today via #1141.
- **Effort**: n/a — audit records are archival; do NOT edit them. Flagged only so future Dim 7 sweeps don't re-flag them. **Information only.**

#### TD8-023 — Dead re-export alias `TreeObjectBounds`

- **Dimension**: Backwards-Compat Cruft
- **Location**: `crates/plugin/src/esm/records/mod.rs:66`
- **Detail**: `pub use tree::{parse_tree, ObjectBounds as TreeObjectBounds, TreeRecord};` aliases `ObjectBounds` for a phantom external consumer. Zero callers anywhere in the workspace.
- **Per CLAUDE.md**: "ByroRedux has no external consumers — delete instead of rename."
- **Effort**: trivial — drop the alias from the re-export line.

#### TD8-024 — Stale `build_skinned_blas` mention in `SKINNED_BLAS_FLAGS` doc comment

- **Dimension**: Backwards-Compat Cruft (orphaned function reference)
- **Location**: `crates/renderer/src/vulkan/acceleration/constants.rs:87`
- **Stale text**: References `build_skinned_blas` (deleted in #1141 today) alongside the two live functions `build_skinned_blas_batched_on_cmd` + `refit_skinned_blas`.
- **Effort**: trivial — strip the orphaned name from the comment.
- **Tracker**: I already updated the cross-reference comment in `blas_skinned.rs` as part of #1141; `constants.rs:87` was missed in that sweep.

#### TD9-200 / TD9-201 — Vulkan-recording files >2000 LOC (BLOCKED, MEDIUM severity unchanged)

- **Dimension**: File Complexity
- **Locations**: `crates/renderer/src/vulkan/context/draw.rs` (2656), `crates/renderer/src/vulkan/context/mod.rs` (2487)
- **Status**: BLOCKED — per `feedback_speculative_vulkan_fixes.md`, splitting Vulkan command-recording / device-init paths without RenderDoc baseline is reckless. Tracked under #1118.
- **Severity**: MEDIUM (not LOW) — these are the only files >2000 LOC and they live on the hot per-frame path.
- **Effort**: large (gated on RenderDoc baseline infrastructure landing first).

### Watch-List Files (1500–2000 LOC, informational only — not regression)

These are the next-most-likely split candidates but are **not regressions**; surfacing them so they don't quietly cross 2000:

| File | LOC | Proposed split axis | Effort |
|------|-----|---------------------|--------|
| `crates/nif/src/blocks/tri_shape.rs` | 1875 | NiTriShape (scene-graph) vs NiTriShapeData (geometry storage) — natural spec boundary | small |
| `byroredux/src/asset_provider.rs` | 1820 | Archive abstraction + texture provider + mesh provider | small |
| `crates/nif/src/import/tests.rs` | 1788 | Tests by block-type / feature module | small |
| `byroredux/src/render.rs` | 1613 | Frame-stage separation (culling / G-buffer / light / composite) | medium |
| `crates/nif/src/blocks/shader.rs` | 1554 | Per-variant dispatcher (NiShader / BSLightingShader / BSEffectShader / Starfield) | small |

---

## Carryover from 2026-05-16 (still present, no regression)

These are unchanged from yesterday — surfaced again as inventory, not new findings:

| ID | Title | Severity | Effort |
|----|-------|----------|--------|
| TD2-201 | Unused doc comment on `shader_constants.rs:1` | LOW | trivial |
| TD2-202 | Unused `VERTEX_STRIDE_FLOATS` import (test-only) | LOW | trivial |
| TD2-203 | `LightHeader.count` field unread in Rust (byte-copied to GPU) | LOW | trivial — add allow + comment |
| TD2-204 | `VF_FULL_PRECISION` aspirational comment in `sse_recon.rs` | LOW | trivial |
| TD4-201 | 32 NIF version-code compares should use `NifVersion::*` | LOW | medium |
| TD4-202 | 142 ESM size literals should map to record constants | LOW | medium |
| TD4-203/204 | BLOOM_INTENSITY / VOLUME_FAR shader↔Rust drift tests missing | LOW | trivial |
| TD5-001 | SpeedTree placeholder billboard (reachable via `--tree`) | MEDIUM | gated on Phase 2 |
| TD5-002 | StencilState parsed but pipeline forces off (#337 Tier 5) | MEDIUM | gated |
| TD5-003 | BSSky/Water shader flags captured, zero renderer consumers | LOW | gated on M33 |
| TD5-008 | IMGS / ACTI / TERM ESM records stubbed (Tier 4) | LOW | gated |
| TD9-202..206 | 5 watch-list files in 1500–2000 LOC band | LOW | small/medium |

---

## Verified Clean (no findings)

- **Dim 1 (Stale Markers)**: zero active TODO/FIXME/HACK/XXX in production code. The 4 grep hits are doc/comment references, not markers.
- **Dim 2 (Dead Code)**: all 25 `#[allow(dead_code)]` markers justified (9 CellLightingRes M41 staging, 6 NIF schema, 2 BA2 mip, 8 misc test/feature/debug).
- **Dim 5 (Stubs)**: `unimplemented!()` / `todo!()` count at zero; 4 carryover stubs all milestone-gated.
- **Dim 6 (Test Hygiene)**: 105 ignored tests sampled (25 examined). All have explicit data-gating, harness-gating, or regression-pinning justification; 0 problematic patterns (smoke-only asserts, commented assertions, unenabled feature gates) found.
- **Dim 9 (Complexity)**: no new files crossed 2000 LOC. The 2 known-deferred (#1118) and 5 watch-list files are tracked.
- **Validate gate** (`.claude/commands/_audit-validate.sh`): PASS — all 287 path refs across 22 skill files valid.

---

## Deferred (gated on in-progress milestones)

| Finding | Gating milestone / decision | Status |
|---------|------------------------------|--------|
| TD5-001 (SpeedTree placeholder) | SpeedTree Phase 2 | Not sequenced |
| TD5-002 (Stencil pipeline) | #337 — Tier 5 renderer polish | Backlog |
| TD5-003 (Sky/Water shader flags) | M33 sky/weather renderer consumer | Backlog |
| TD5-008 (IMGS/ACTI/TERM records) | Tier 4 interactivity | Backlog |
| TD9-200 / TD9-201 (Vulkan-recording files >2000 LOC) | RenderDoc baseline infrastructure | Blocked per #1118 |
| CellLightingRes staged fields (9 × `allow(dead_code)`) | M41 shader-side consumer landing (#865, #861) | In progress per shader |
| TD10-001 (80 stale local issue files) | Workflow decision: canonical / advisory / immutable | **Pending decision** |

---

## Recommendations

1. **Ship the quick-wins batch** (~2.5 h, ~30 LOC): TD4-301, TD4-302, TD4-303, TD7-051, TD8-023, TD8-024, TD3-207. All trivial, mechanical, no test surface impact. Single PR or thin batched commit.

2. **Resolve TD10-001 workflow decision** before next audit. The Dim 10 finding is the only systemic problem; everything else is line-item cleanup. Without a decision, the next audit will rediscover the same 80+ stale files and burn agent tokens on `gh issue view` cross-checks per finding.

3. **Continue watch-listing** the 5 files in the 1500–2000 LOC band. Session 36 made progress; another sweep when one crosses the threshold is fine.

4. **TD9-200 / TD9-201**: hold the line per `feedback_speculative_vulkan_fixes.md`. Don't split until RenderDoc baseline exists.

---

## Next Step

```
/audit-publish docs/audits/AUDIT_TECH_DEBT_2026-05-17.md
```

This will file the 9 LOW + 1 MEDIUM new findings as GitHub issues with `tech-debt` + `maintenance` labels (per the audit-publish skill).
