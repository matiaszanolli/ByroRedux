# NIFAL Audit — 2026-08-24

Scope: all 9 dimensions, all games. Solo (non-fanned-out) run, one day after
`AUDIT_NIFAL_2026-08-23.md`. Executed by reading/grepping/tracing directly
against the live tree rather than dispatching per-dimension sub-agents.

## Executive Summary

**0 NEW findings this sweep.** All three real findings from the 2026-08-23
sweep (2 HIGH, 1 MEDIUM) plus its LOW doc-rot finding have landed fixes in
the 24 hours since, and this sweep verified each fix against the live code
and its regression tests rather than trusting the commit messages. No new
canonical-boundary defect was found across the small diff surface that
touched NIFAL-relevant paths since that sweep.

One item from today's other audits was cross-checked for NIFAL relevance and
is **not** re-reported here because it belongs to a different boundary:
`crates/core/src/animation/player.rs::advance_time`'s `CycleType::Loop` NaN
latch on unvalidated `clip.frequency` is already filed as
`ECS-2026-08-24-11` in `AUDIT_ECS_2026-08-24.md`. Its root cause does touch
the Dimension 7 translate boundary (`byroredux/src/anim_convert.rs:494`
copies `nif.frequency` straight through with no finiteness/sign validation,
from both `crates/nif/src/anim/entry.rs:344` and `sequence.rs:22`), so this
report cross-references it below rather than filing a duplicate.

The other flagged item — commit `4e1afcbe`'s deletion of the geometry-mesh
SpeedTree wind-bend loop in `byroredux/src/systems/billboard.rs` — was
checked and does **not** belong to NIFAL: `tree_bones` (the NIFAL-tracked
parked field, `BSTreeNode` branch/trunk bone names) is not what feeds
`SpeedTreeWind` on cell-loaded mesh entities. `SpeedTreeWind` is instead
populated from a scene-import-time cache (`cached.speedtree_wind`,
`byroredux/src/cell_loader/nif_import_registry.rs`) that this same 24-hour
window (`4e1afcbe`) also changed to a fixed neutral placeholder
`(1.0, 0.0)` rather than a guessed CNAM-float decode — a real no-fabrication
fix, but for `.spt`/tree-record parsing (`crates/spt`, `crates/plugin`
`TREE` record), not a NIF-import canonical-translation boundary. Whether the
deleted geometry-branch consumer leaves any live `SpeedTreeWind`+`MeshHandle`
(no-`Billboard`) entity unbent is a real question, but it is a downstream
render-system question, not a translate()-boundary one — see the open
SpeedTree audit thread (`#3190`–`#3195`, esp. `#3193`) for the right owner.
Flagged here for the record, not filed as a NIFAL finding.

## Per-Category Tier Matrix

| Category | single-boundary | no-fabrication | no-leak | no-render-time-fallback | Notes |
|---|---|---|---|---|---|
| Material (Dim 1) | PASS (3 documented callers, all through `translate_material`; `byroredux/src/cornell.rs:1783` is a 4th, synthetic-harness caller — same boundary, not a violation) | PASS | PASS | PASS | unchanged from 08-23 |
| Geometry/Transform (Dim 2) | PASS | PASS | PASS | PASS | no relevant commits since 08-23; unchanged |
| Skinning (Dim 3) | PASS | — | documented gap (#2440, unchanged) | — | unchanged |
| Lights (Dim 3) | PASS | **PASS — FIXED** | — | — | NIFAL-D3-2026-08-23-01 verified fixed, see below |
| Nodes (Dim 4) | N/A by design | — | PASS (7 parked fields, 0 canonical consumers) | — | unchanged |
| Particles (Dim 5) | PASS (`apply_emitter_overlays`) | PASS | PASS | PASS | unchanged; #2610 still open, not re-reported |
| Collision (Dim 6) | PASS | PASS | PASS | — | 16/16 `bhk*Shape` resolve arms reconfirmed via grep against `dispatch_coverage_tests`' own count |
| Animation (Dim 7) | PASS (`convert_nif_clip`) | — | **PASS — FIXED** (morph index) | — | NIFAL-D7-2026-08-23-01 verified fixed; unvalidated `frequency` noted below (cross-ref, not filed here) |
| Shader flags/texture roles (Dim 8) | **PASS — FIXED** | — | **PASS — FIXED** | PASS (zero `if game ==` in `triangle.frag` + `include/*.glsl`, reconfirmed) | NIFAL-D8-2026-08-23-01 verified fixed; `values()`/struct field parity reconfirmed (18/18) |
| Completeness signal (Dim 9) | — | — | — | PASS | NIFAL-D9-2026-08-23-01 verified fixed (floors now use `assert_pbr_override_fill`, matching #2707); NIFAL-D9-2026-08-23-02 verified fixed (SKILL.md now lists 3 callers) |

## Findings

No new findings this sweep. See **Fix Verification** below for the four
items closed since yesterday, and the **Documented-Limitation Ledger** for
what remains open by design.

## Fix Verification (re-derived against live code + tests, not trusted from commit messages)

### NIFAL-D3-2026-08-23-01 — light direction rotation — CONFIRMED FIXED
`byroredux/src/cell_loader/spawn.rs:920-923` now computes
`let world_direction = (ref_rot * Vec3::new(light.direction[0], light.direction[1], light.direction[2])).to_array();`
before constructing `LightSource::from_legacy_world_units`, mirroring
`systems/light_anim.rs::translate_light`'s established pattern. Pinned by
`byroredux/src/cell_loader/nif_light_spawn_gate_tests.rs::spawn_nif_lights_rotates_direction_by_reference_rotation`
(passing: `cargo test -p byroredux --bin byroredux nif_light_spawn_gate_tests::`,
15/15 green).

### NIFAL-D7-2026-08-23-01 — morph-target index desync — CONFIRMED FIXED
`crates/nif/src/import/mesh/morph.rs::extract_morph_targets` now enumerates
`data.morphs` and stamps each surviving `ImportedMorphTarget` with its
`original_index: u32` (the source array position, not the compacted `Vec`
position); `mesh_instance.rs::flatten_morph_targets` sizes the GPU delta
buffer off `target.original_index + 1` and writes each target at
`original_index * vertex_count`, so a filtered-out malformed target leaves an
inert hole instead of shifting every later target down by one — matching
`resolve_morph_target_index`'s (`crates/nif/src/anim/channel.rs`) index
convention, which was never touched. This closes the finding *before* it
could go live: `d0322785` (Phase D, GPU morph consumer wiring) landed the
evening of 2026-08-23, and the index fix (`06f86742`, morning of
2026-08-24) landed before any release build shipped with the two boundaries
disagreeing. Both `crates/nif`'s own unit tests
(`drops_target_with_mismatched_vertex_count` now asserts index `2` survives
a dropped index `1`) and `mesh_instance.rs`'s
`morph_gpu_buffer_preserves_filtered_source_index_holes` pass
(`cargo test -p byroredux-nif --lib morph::`, 7/7 green).

### NIFAL-D8-2026-08-23-01 — BGSM specular/smooth_spec role swap — CONFIRMED FIXED
`RefrTextureOverlay` (`byroredux/src/cell_loader/refr.rs`) now carries a
separate `external_specular` field alongside `smooth_spec`, and
`fill_from_bgsm`'s `.bgsm` arm fills `smooth_spec` from
`f.smooth_spec_texture` and `external_specular` from `f.specular_texture` —
previously the latter read was missing entirely and `smooth_spec_texture`
alone fed the role BGSM's real `specular_texture` should have owned. The
regression-pinning test file was also corrected:
`fill_from_bgsm_forwards_every_bgsm_texture_role` and
`fill_from_bgsm_forwards_every_bgem_texture_role` in
`refr_texture_overlay_tests.rs` now exercise both fields distinctly
(`cargo test -p byroredux --bin byroredux refr_texture_overlay_tests::`,
23/23 green).

### NIFAL-D9-2026-08-23-01 — stale completeness floors — CONFIRMED FIXED
`crates/nif/tests/translation_completeness.rs` now calls
`assert_pbr_override_fill(s, label, <per-game floor>)` for all seven games
(80.0–90.0 range, content-dependent) instead of a blanket `>= 99.9`, with a
comment block explicitly citing #2707's no-fabrication fix as the reason
`metO`/`rghO` are content-dependent signals again, not "100% by
construction."

### NIFAL-D9-2026-08-23-02 — SKILL.md stale caller count — CONFIRMED FIXED
`.claude/commands/audit-nifal/SKILL.md:109` now lists all three production
callers (`scene/nif_loader.rs`, `cell_loader/spawn/mesh_instance.rs`,
`cell_loader/placement_lod.rs`) with "MUST all route through this one fn"
phrasing, matching the Particles/Animation dimensions' established wording.

## Cross-Audit Note (not filed here — see rationale in Executive Summary)

**ECS-2026-08-24-11** (`AUDIT_ECS_2026-08-24.md`) — `advance_time`'s
`CycleType::Loop` arm can latch `local_time` to `NaN` from an unvalidated
`clip.frequency` (MEDIUM). The raw value crosses two independent NIFAL Dim-7
producers unvalidated — `crates/nif/src/anim/entry.rs:344`
(`clip.frequency = b.frequency;`, embedded-controller path) and
`crates/nif/src/anim/sequence.rs:22` (`let frequency = seq.frequency;`, KF
path) — and then the single `convert_nif_clip`/`anim_convert.rs:494`
boundary (`frequency: nif.frequency`) copies it into the canonical
`AnimationClip` with no `is_finite()`/sign check anywhere in between. This is
architecturally a Dimension-7 "no-leak" gap (an un-sanitized raw scalar
reaching the canonical type), and the ECS audit's own suggested fix already
recommends resolving it at this exact boundary
("`anim_convert.rs:494`: `frequency: if nif.frequency.is_finite() &&
nif.frequency > 0.0 { nif.frequency } else { 1.0 }`") rather than only
defending in `advance_time`. Recorded here so a future NIFAL sweep does not
have to rediscover the boundary-level angle; the finding itself stays owned
by the ECS audit to avoid a duplicate GitHub issue.

## Documented-Limitation Ledger (re-verified this cycle, not re-reported)

- Skinning cell-loader gap (#2440) — cell-placed skinned geometry (non-NPC)
  still renders in bind pose; unchanged.
- Node passthroughs (`bs_value_node`, `bs_ordered_node`, `tree_bones`,
  `range_kind`, `lod_group`, `bs_lod_cutoffs`, `bs_sub_index`) — all still
  zero canonical consumers; unchanged. Note: `tree_bones` (SpeedTree
  branch/trunk bone *names*, parsed from `BSTreeNode`) remains distinct from
  and unrelated to the `.spt`/TREE-record CNAM wind-parameter question
  raised in the Executive Summary — do not conflate the two SpeedTree gaps.
- `BhkNPCollisionObject`/`BhkPCollisionObject` phantom/packed-Havok
  limitations — unchanged, still accurately documented.
- `#2610` — particle `DrawCommand.effect_shader_flags` still hardcoded `0`.
- `#3187` — `RefrTextureOverlay::apply_slot_swap` is still a game-agnostic
  flat slot table (FO4 slot-5 reads the wrong lane); unchanged, unrelated to
  this cycle's verified fixes.
- SLSF1 `Refraction`-without-`Fire_Refraction` (#2327 / SKY-D7-02) — still
  has no canonical field/shader consumer for ordinary refractive
  glass/ice/crystal; documented deliberate gap, unchanged.
- `grayscale_to_palette_scale` (#2443, MAT-D3-01) — still captured on
  `Material` but unshaded (`triangle.frag`'s palette branch is still an
  unmodulated direct lookup); unchanged, not a boundary leak.

## Verification Method

- Read `docs/engine/nifal.md` (full) and the last three `AUDIT_NIFAL_*.md`
  reports (`2026-08-23`, `2026-08-20`, `2026-08-16`) before touching code, to
  avoid re-deriving already-settled ground truth.
- Diffed `git log 5db3b0b9..HEAD` (the commit the 08-23 report was written
  against, through current `HEAD`) against every NIFAL-relevant path listed
  in `_audit-common.md`'s project layout, then read the full diff of every
  commit that touched one.
- Re-ran the specific regression tests each verified fix depends on
  (`cargo test -p byroredux-nif --lib morph::`; `cargo test -p byroredux
  --bin byroredux refr_texture_overlay_tests::`; `cargo test -p byroredux
  --bin byroredux nif_light_spawn_gate_tests::`) rather than trusting the
  commit message claims of "tests pass."
- `cargo check -p byroredux-nif -p byroredux-core -p byroredux` — clean
  (workspace-bare `cargo test` is known-broken today on an unrelated
  `crates/scripting/examples/fragment_coverage.rs:59` E0004, per session
  notes; per-crate checks used instead).
- Reconfirmed `triangle.frag` + `include/*.glsl` carry zero `if game ==`
  branches, and `MaterialTextureSet::values()`'s 18-entry array still
  matches the struct's 18 named fields 1:1 (the one hand-written role walk
  the compiler does not protect, per Dimension 8's own checklist).

Suggest: `/audit-publish docs/audits/AUDIT_NIFAL_2026-08-24.md` (nothing to
publish this cycle — all findings already tracked/fixed or owned by another
report).
