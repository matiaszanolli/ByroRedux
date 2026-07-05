# Tech-Debt Audit — 2026-07-05

**Scope**: All 9 dimensions, `--depth deep`. Full codebase (`crates/` 21 crates + `byroredux/`).
**HEAD**: `96c94627`. **Prior baseline**: `AUDIT_TECH_DEBT_2026-07-03.md` @ `8498e559`.
**Method**: Discovery recipes per `.claude/commands/audit-tech-debt/SKILL.md`; every
finding re-verified against live source. Dedup baseline = `gh issue list`
(all-state, `tech-debt`-labelled 500-issue window, 196 issues) + prior
`AUDIT_TECH_DEBT_*.md` reports.

## Executive Summary

The codebase remains in **very clean shape**. 33 commits landed between the
07-03 sweep (`8498e559`) and this one (`96c94627`) — targeted bug fixes with
regression tests (#1783–#1831 audit bug-bash tail), one file split (#1869),
CHARAL documentation work, and this session's #1873/#1832 fixes. No new
markers, stubs, dead code, or `#[ignore]` tests landed.

**One regression since 07-03**: the path-validation gate flipped from GREEN to
**RED**. The #1869 `resources.rs` split (which *resolved* the sole 07-03
finding) left 7 backticked path refs dangling in 3 audit-skill files — the
exact TD7-*/#1114 stale-path class the gate exists to catch. This is the
headline finding.

Findings this pass: **3 total** — 0 CRITICAL, 0 HIGH, 1 MEDIUM, 2 LOW.

| Severity | Count | IDs |
|----------|-------|-----|
| CRITICAL | 0 | — |
| HIGH | 0 | — |
| MEDIUM | 1 | TD3-2026-07-05-01 (path gate RED — 7 stale `resources.rs` refs) |
| LOW | 2 | TD1-2026-07-05-01 (`collision.rs` 2587 LOC), TD1-2026-07-05-02 (`references.rs` 2078 LOC) |

### Delta vs baseline (2026-07-03, commit `8498e559`)

| Metric | 07-03 | 07-05 | Note |
|--------|-------|-------|------|
| Marker total | 17 | 17 | unchanged, all false positives (protocol `XXXX`, ref-impl FIXME docs, closed-#242 TODO in `scene.rs`) |
| `allow(dead_code)` | 20 | 20 | unchanged; TD8-004/#1761 still open, not re-reported |
| `unimplemented!/todo!()` | 0 | 0 | unchanged — engine still prefers explicit fallbacks |
| `#[ignore]` tests (`*.rs` only) | 134 | 134 | unchanged — no new ignore-gated tests |
| files >2000 LOC | 7 | 7 | same count, membership shifted (see below) |
| **path gate** | **GREEN (1006 refs)** | **RED (7 stale)** | **regression — #1869 split left dangling refs** |

**Oversized-set membership turnover** (count held at 7, but composition changed):
- **Left the set**: `crates/core/src/ecs/resources.rs` (was 2077) — split into
  `resources/mod.rs` (1210) + `resources/skin_slot_pool.rs` (875) by #1869
  (`2d823f11`). This **resolved the sole 07-03 finding** (TD1-2026-07-03-01).
- **Joined the set**: `byroredux/src/cell_loader/references.rs` (2078) — crossed
  via the `9107dfa1` door-spawn-selection commit (TD1-2026-07-05-02 below).
- **Grew materially**: `crates/nif/src/import/collision.rs` 2370→2587
  (this session's #1832 fix, TD1-2026-07-05-01 below); `draw.rs` 4411→4607,
  `context/mod.rs` 3348→3452, `main.rs` 2884→2955 (all have open split issues
  #1857/#1749/#1858, not re-reported).

## Baseline Snapshot (for next audit to diff)

```
TODO/FIXME/HACK/XXX:    17   (all false positives — protocol / ref-impl docs / closed-#242 TODO)
allow(dead_code):       20   (19 justified, 1 stale — tracked as #1761/TD8-004)
unimplemented!/todo!(): 0
#[ignore] tests (*.rs only, code-accurate): 134
files >2000 LOC:        7
path gate:              RED — 7 stale refs (TD3-2026-07-05-01)
```

Oversized set (live, today):
```
4607  crates/renderer/src/vulkan/context/draw.rs        (Existing: #1857 / TD1-001)
3452  crates/renderer/src/vulkan/context/mod.rs          (Existing: #1749 / TD1-004, new() ctor)
2955  byroredux/src/main.rs                              (Existing: #1858 / TD1-003)
2587  crates/nif/src/import/collision.rs                 (TD1-2026-07-05-01, this report — grew +217)
2166  crates/nif/src/blocks/particle.rs                  (pre-existing, no open split issue)
2078  byroredux/src/cell_loader/references.rs            (TD1-2026-07-05-02, this report — NEW crossing)
2065  crates/plugin/src/esm/records/actor.rs             (pre-existing, no open split issue)
```

## Top Quick Wins

1. **TD3-2026-07-05-01** (trivial) — re-point 7 stale `resources.rs` refs → `resources/mod.rs`; turns the path gate GREEN again and unblocks `/audit-publish`.

## Findings

### MEDIUM

#### TD3-2026-07-05-01: Path gate RED — 7 stale `crates/core/src/ecs/resources.rs` refs after the #1869 split
- **Severity**: MEDIUM (promotion: stale audit-skill baseline that would misdirect any agent following those dimensions; the `_audit-validate.sh`/#1114 gate — built for exactly this — is now failing)
- **Dimension**: 3 (Stale Documentation) / 4 (Audit-Finding Rot)
- **Location**: `.claude/commands/audit-performance/SKILL.md` (lines 78, 84, 113, 116, 140), `.claude/commands/audit-starfield/SKILL.md` (line 199), `.claude/commands/audit-tech-debt/SKILL.md` (line 113)
- **Status**: NEW
- **Age**: introduced by `2d823f11` (Fix #1869, split `SkinSlotPool` out of `resources.rs`) — landed after the 07-03 audit, which recorded a GREEN gate at 1006/1006 refs.
- **Description**: `crates/core/src/ecs/resources.rs` no longer exists; it is now the directory `crates/core/src/ecs/resources/` (`mod.rs` + `skin_slot_pool.rs`). Seven backticked refs across three audit-skill files still point at the vanished file, so `.claude/commands/_audit-validate.sh` now exits 1. This is the recurring TD7-* stale-path class the gate was created to catch; it also means the gate blocks `/audit-publish`'s own path-validation step until fixed.
- **Evidence**: `.claude/commands/_audit-validate.sh` output — `STALE: … resources.rs` ×7, `FAIL: 7 stale path reference(s)`.
- **Impact**: any agent running `/audit-performance` or `/audit-starfield` is directed to a nonexistent file; `/audit-publish` path gate fails on every report until fixed.
- **Related**: Same class as the original #1114 / TD7-050 that motivated the gate. Distinct from #1761 (dead_code, unrelated).
- **Suggested Fix**: Re-point each ref. Most target the resource *domain* generally → `crates/core/src/ecs/resources/mod.rs`; if a ref names `SkinSlotPool` specifically, point it at `crates/core/src/ecs/resources/skin_slot_pool.rs`. Re-run `_audit-validate.sh` to confirm GREEN.
- **Effort**: trivial

### LOW

#### TD1-2026-07-05-01: `crates/nif/src/import/collision.rs` is 2587 LOC (grew +217, no open split issue)
- **Severity**: LOW
- **Dimension**: 1 (File Complexity)
- **Location**: `crates/nif/src/import/collision.rs`
- **Status**: NEW (noted-but-unfiled in 07-03 at 2370 LOC; grew materially this session, still no tracking issue)
- **Age**: crossed 2000 pre-07-03; grew 2370→2587 via `ae083d69` (this session's #1832 zero-mass-Dynamic fix + its diagnostic logging).
- **Description**: 4th-largest file in the workspace, no open split issue. The block-parser side already split by bhk shape family under `crates/nif/src/blocks/collision/` (mod / collision_object / rigid_body / ragdoll / shape_primitive / shape_compound / shape_mesh / compressed_mesh / constraints / phantom_action); the *import* side (`resolve_shape_inner`, `extract_from_*`, `extract_ragdoll`, the diagnostic helpers) has not.
- **Evidence**: `wc -l crates/nif/src/import/collision.rs` → 2587.
- **Impact**: every collision-import edit (a recurring hot path — this session touched it twice) pays the whole-file tax.
- **Related**: sibling of the closed Session-34/35 splits; no open issue. Mirrors the `blocks/collision/` split axis.
- **Suggested Fix**: Split by shape family mirroring `crates/nif/src/blocks/collision/` — e.g. `import/collision/{mod, shape_resolve, rigid_body, ragdoll, diagnostics}.rs`. Keep the `#[cfg(test)]` dispatch/cycle/coord test modules with their owning submodule.
- **Effort**: medium

#### TD1-2026-07-05-02: `byroredux/src/cell_loader/references.rs` crossed 2000 LOC (2078, new)
- **Severity**: LOW
- **Dimension**: 1 (File Complexity)
- **Location**: `byroredux/src/cell_loader/references.rs`
- **Status**: NEW
- **Age**: crossed via `9107dfa1` (door-spawn-point selection) landing on top of prior growth; first appearance in the >2000 set.
- **Description**: `load_references` and its per-REFR placement/collision/light/SCOL-child handling now exceed the Session-34 split threshold. No open split issue.
- **Evidence**: `wc -l byroredux/src/cell_loader/references.rs` → 2078.
- **Impact**: the REFR-placement hot path (touched by cell-load, precombine, spawn-point work) taxes every edit.
- **Related**: part of the `cell_loader/` dispatcher family already split into per-feature submodules; this one re-bloated.
- **Suggested Fix**: Extract by responsibility — e.g. REFR placement/transform composition vs SCOL child expansion vs spawn-point selection (the `door_pos` precedence logic) vs the per-REFR light/collision attach. The spawn-point block is a natural first cut (self-contained, recently added).
- **Effort**: medium

## Verified Clean (no finding)

- **Markers** — 17, all false positives: protocol `XXXX` tag (`esm/reader.rs`, `records/misc/magic.rs`), ref-impl FIXME documentation (`bgsm/src/bgem.rs`, `nif/blocks/bs_geometry.rs`), and a closed-#242 TODO breadcrumb in `byroredux/src/scene.rs:791` (reads "Closes the #242 consumer-side TODO" — descriptive, not open work).
- **Stubs** — `unimplemented!/todo!()` still 0.
- **Dead code** — 20 `allow(dead_code)`, unchanged; the one stale (`Dx10Chunk::start_mip`) remains tracked as **#1761** (OPEN), not re-reported.
- **`#[ignore]` tests** — 134 (`*.rs`), unchanged; all Vulkan/smoke/game-data gated.
- **`docs/feature-matrix.md`** — the M45 save/load and M47.2 transpiler rot the skill flags is **already fixed**: the Save/load row was removed 2026-06-21 (TD3-002, note preserved at line 169) and the M47.2 row now correctly reads "✓ `.pex` recognizer slice … full transpiler deferred". The skill text itself is stale on this point (see note below), but the doc is clean.
- **GPU-struct sizes** — `GpuInstance` 112 B, `GpuCamera` 336 B pinned by `gpu_instance_layout_tests.rs`; this session's #1873 change added `specular_authored: bool` to `PbrClassifierInputs` (a CPU-side classifier input struct, **not** a `#[repr(C)]` GPU struct), so no layout drift.
- **`Material::classify_pbr` doc refs** — all 6 doc-comment mentions in `material.rs` correctly frame it as deleted/historical ("the per-draw fallback that was removed", "the deleted `Material::classify_pbr`"); surviving symbols `classify_pbr_keyword` + `Material::resolve_pbr` unchanged.
- **Duplication** — this session's new `aabb_extent`/`shape_size_descriptor` helpers in `collision.rs` are diagnostic-only (feed a `log::debug!` string); `aabb_extent` overlaps conceptually with `crates/core/src/ecs/components/local_bound.rs` but is a 10-line local in a different crate for logging, not a consolidation target.

## Skill-text drift noted (not a repo finding)

`.claude/commands/audit-tech-debt/SKILL.md` Dimension 3 still instructs auditors
that `docs/feature-matrix.md`'s "Save / load (M45)" row "reads unstarted" and
M47.2 "reads transpiler unstarted" — both were corrected in the doc on
2026-06-21. The skill's own guidance is now stale on this point (harmless — it
points at already-fixed rot). Fold into the TD3-2026-07-05-01 skill-refresh pass
if convenient. (Also: line 113 of this skill is itself one of the 7 stale
`resources.rs` refs.)

## Deferred

None. All three findings are actionable now; no in-progress milestone gates any.
