# Legacy Compatibility Audit — 2026-07-25

**HEAD:** ca7a4e0e · **Type:** legacy-compat (canonical-translation boundary pass) ·
**Run as:** one leg of the `comprehensive` audit-suite sweep

**Scope:** Compatibility/mapping gaps between Gamebryo 2.3 / Creation-engine
behaviour and Redux, framed by the three canonical translation layers
(NIFAL / EXAL / PHYSAL) plus coordinate-system correctness, the per-game
translation survey, and subsystem coverage vs. the legacy headers (all 7
dimensions of `.claude/commands/audit-legacy-compat/SKILL.md`).

**Method:** Delta pass over the 2026-07-16 report (base c3e09bb5 → HEAD
ca7a4e0e, 106 commits). Re-verified all four single-producer boundaries
(material / env / coord / ragdoll) by grepping and reading every touched
file at HEAD, not just trusting commit messages. Read `docs/engine/nifal.md`,
`docs/engine/physal.md`, and the 07-16 report in full as the leak-inventory
baseline. Ran the mandatory dedup step (`gh issue list` → 29 currently-open
issues saved to `/tmp/audit/issues.json`, cross-checked against
`docs/audits/`) before writing anything up. Every candidate finding below
was traced to its call sites and read in full before being accepted or
rejected — several near-misses were disproved (see "Disproved candidates").

**Headline: zero new findings.** All four canonical boundaries remain
single-producer clean at ca7a4e0e. The one open finding from the prior pass
(`LC0716-01`, the Skyrim+ `PACK.PSDT` 8-byte-vs-12-byte schedule offset bug)
is now **fixed and closed** (`55ae73e2`, tracked as #2012/#2014/#2015),
verified in place with an era-branching test
(`parse_pack_reads_skyrim_plus_psdt_schedule_from_offset_8`). The 106-commit
window since the last pass was dominated by renderer work (FSR 3.1
upscaler, decal alpha-blend handling, directional-light/shadow refactor,
surface-ID plumbing) rather than compat-surface work, and none of it
introduced a new leak, a duplicated coordinate swap, or a per-game branch
downstream of a translate boundary.

---

## Boundary verification results (no findings — recorded for the trail)

| Layer | Claim verified | Result |
|---|---|---|
| **Coordinate system** | `(x,z,-y)` swap + `EXTERIOR_CELL_UNITS` single-source | **Clean.** `git diff c3e09bb5..HEAD` has zero hits for a new `4096.0` cell-math literal or a re-derived `zup_to_yup_*` swap outside `crates/core/src/math/coord.rs`. `terrain_lod.rs` (TD2-101, previously flagged in a tech-debt pass) now calls `zup_to_yup_pos` directly (`byroredux/src/cell_loader/terrain_lod.rs:29,469,489,505`) with a regression test (`zup_to_yup_pos_matches_old_inline_swizzle`) pinning it against the old inline swizzle. `DalcCubeYup::from_skyrim_zup` (`byroredux/src/components.rs:702`) still hand-spreads the same permutation across 6 named cube-face fields rather than calling the helper directly — this is the pre-existing, already-cross-referenced (#2062) exception documented in its own doc comment (structurally can't be a single `[f32;3]` call site), not a new duplication. |
| **NIFAL — material** | `translate_material` sole populated-`Material` producer | **Clean.** Only non-test callers remain `scene/nif_loader.rs` and `cell_loader/spawn.rs`. A new `ior: f32` field was added to `Material` this window (`crates/core/src/ecs/components/material.rs:13,256`, `DEFAULT_DIELECTRIC_IOR = 1.5`) — traced its full producer chain: `translate_material` seeds the default, `classify_glass_into_material`'s single call to `apply_surface_behavior(GLASS_SURFACE_BEHAVIOR)` (`byroredux/src/helpers.rs:109`) is the only override site, and the one other place `DEFAULT_DIELECTRIC_IOR` appears (`render/static_meshes.rs:342`) is the pre-existing "no `Material` component at all" fallback tuple (same pattern already used for `roughness`/`metalness` defaults there) — not a second producer. New field, same single-boundary contract. |
| **NIFAL — decal/glass alpha classification** | `effective_alpha_blend` / `classify_glass_into_material` stay single-boundary across all geometry formats | **Clean.** `a09d2b76` + `388b9969` reworked how alpha-test-only glass (FNV pitchers/glasses) and FO4 decals get their blend state. `MaterialInfo::effective_alpha_blend` is called from exactly the four geometry-extraction sites (`ni_tri_shape.rs`, `bs_tri_shape.rs`, `bs_geometry.rs`, `precombine.rs`), each invoking the same shared method rather than reimplementing the glass-keyword check — consistent with the established "one function, N format-specific call sites" pattern already used for tangent extraction and vertex decode. `decal_uses_implicit_alpha_blend` is likewise one shared helper called identically from both spawn paths (`cell_loader/spawn.rs:1216`, `scene/nif_loader.rs:836`). No per-game branch in either. |
| **EXAL — env resources** | `env_translate.rs` sole `SkyParamsRes`/`WeatherDataRes`/`WaterMaterial` producer | **Clean.** The only diff in this window is new test coverage (`resolve_water_material_procedural_default_classification`, a regression pin for #1997/REN-D15-01's four procedural-fallback cases) plus `cargo fmt` reflow — zero logic changes to the boundary itself. |
| **PHYSAL — ragdoll** | one translate, one build; extract game-agnostic | **Clean.** `crates/nif/src/import/collision/ragdoll.rs` diff is `cargo fmt` only — still zero `game ==` branches, still switches only on `BhkConstraintData`. `byroredux/src/ragdoll.rs`'s 172-line diff is the already-tracked-and-closed #2083 fix (re-activating a ragdoll now frees the previous Rapier body/joint set via `pw.remove_ragdoll(old)` before building the new one) — an activation-lifecycle leak fix, orthogonal to and not a violation of the "one translate, one build" contract. Verified with its own regression test (`reactivating_ragdoll_does_not_leak_previous_bodies`). |
| **Renderer per-game branch** | render side carries no `if game == …` | **Clean.** Zero hits for `game ==`/`GameKind::`/`is_skyrim`/`is_fo4`/`is_starfield` across `crates/renderer/src` + `byroredux/src/render`, despite the large `context/draw.rs` decomposition into `geometry_pass.rs` / `post_passes.rs` / `skinned_blas_refit.rs` this window (2367 lines moved, not rewritten with per-game logic). |
| **`PACK` PSDT era gap (LC0716-01)** | Skyrim+ 12-byte schedule layout now correctly gated | **Fixed, verified in place.** `crates/plugin/src/esm/records/misc/pack.rs::parse_pack` now takes `game: GameKind` and branches `duration_offset` on `game.uses_prebaked_facegen()` (8 vs. 4), with a dedicated test (`parse_pack_reads_skyrim_plus_psdt_schedule_from_offset_8`) proving the two eras genuinely diverge on the same input bytes. `misc/ai.rs` was split into per-family files (#2054) — `parse_pack` now lives in `crates/plugin/src/esm/records/misc/pack.rs`; the sibling `PACK` dispatch arm in `records/mod.rs` and the `is_scol_era`/`is_fo4_plus` gating precedent for `SCOL`/`PKIN`/`MOVS`/`MSWP` are both still present and unchanged. |

---

## Findings

None. Every candidate surfaced by the delta review either turned out to be
`cargo fmt`-only reflow, a already-tracked-and-closed issue fix, or a
change that stays inside its category's existing single-boundary contract
(see table above for the trail on each).

### Disproved candidates (recorded per `_audit-common.md` methodology — "attempt to disprove before including")

- **"New `Material.ior` field is a second-producer risk"** — disproved:
  traced every write site; exactly one default assignment
  (`translate_material`) and one override (`classify_glass_into_material`'s
  `apply_surface_behavior` call); the third occurrence is the pre-existing
  no-Material rendering fallback, not a `Material` construction site.
- **"`is_decal` parameter change in `classify_glass_into_material`
  (`mesh.is_decal || mesh.alpha_test` → `mesh.is_decal`) silently drops the
  alpha-test decal exclusion, letting decals get misclassified as glass"**
  — disproved: `has_transparent_coverage` (the caller's `has_alpha`
  argument) was *simultaneously* widened to `mesh.has_alpha ||
  mesh.alpha_test` in the same commit (`a09d2b76`), so alpha-test-only
  glass (which previously needed the `is_decal` OR-hack to *avoid* the
  decal gate) now reaches the glass classifier through the transparency
  gate instead — a coherent single-boundary refactor, not two independent
  changes that happen to compensate for each other. Read
  `classify_glass_into_material`'s full body (`byroredux/src/helpers.rs:63-104`)
  to confirm the gate ordering still makes semantic sense (decal gate
  fires *before* the metalness/BGEM checks, same as before).
- **"`DalcCubeYup::from_skyrim_zup` hand-derives the coordinate swap —
  re-file as a coordinate-system duplication (TD2-102 redux)"** — disproved:
  this is not new; it is the same pre-existing, already-cross-referenced
  (#2062) exception the struct's own doc comment explains (a 6-named-field
  cube can't be a `[f32;3]` call site to `zup_to_yup_pos`). No change to
  this function in the audited window.
- **"`context/draw.rs`'s 2367-line decomposition into `geometry_pass.rs` /
  `post_passes.rs` / `skinned_blas_refit.rs` might have introduced a
  per-game branch or duplicated a translate-boundary call while moving
  code"** — disproved: grepped the full new file set for `game ==` /
  `GameKind` / material-construction patterns; all zero. This is a
  pure move+split, in scope for `/audit-renderer` and `/audit-tech-debt`,
  not a legacy-compat regression.

---

## Still-open tracked gaps re-verified (Existing — do not re-file)

Re-checked against the current `gh issue list` snapshot
(`/tmp/audit/issues.json`, 29 open) and the live code; all remain accurate
and unchanged since 07-16 except where noted:

- **#1849** (LC0702-05) — WRLD `NAM3`/`NAM4` LOD-water + `OFST` cell-offset
  table skipped. OPEN, unchanged (EXAL §5.4).
- **#1856** (FO3-D1-01) — FO3 `WaterShaderProperty.water_shader_flags`
  dead-ends at `MaterialInfo`. OPEN, NIFAL passthrough, unchanged.
- **#1827** (FO4-D4-02) — Starfield `BSGeometry` leaves per-vertex bone
  indices/weights empty. OPEN, informational (Starfield-audit scope).
- **#1576** (SF-D4-03) — Model-less STAT/BNDS/ACTI/ARMO Starfield forms
  drop (geometry lives in a BFCB component block). OPEN, unchanged.
- **#1981** (FNV-D7-02) — Skinned-mesh `WorldBound` does not track a
  ragdoll that leaves its origin (cull/RT-bound pop). OPEN, unchanged;
  not touched by this window's #2083 ragdoll-leak fix (that fix is about
  Rapier body/joint lifecycle, not `WorldBound` derivation).
- **#2098** (SF2D2-01) — `BSGeometry` bounding-sphere scale not
  cross-checked against havok-scaled vertices. OPEN, Starfield-audit scope.
- **#2099** (SF2D2-02) — Secondary UV channel (`uvs1`) parsed then dropped
  by the importer. OPEN, NIFAL passthrough, Starfield-audit scope.
- **#2108** (SF-D9-01) — `EFFECT_PALETTE_COLOR/ALPHA` derived from
  LUT-texture presence, not the authored palette-enable flag. OPEN.
- **#2109** (SF-D9-02) — BGEM v21/v22 glass-overlay params +
  envmap-mask-scale + v11 emittance dropped in merge. OPEN, NIFAL
  passthrough (material boundary), Starfield-audit scope.
- **#1822** (SPT-NEW-07) — `MaybeStringElseBare` (tag 13005) misparse risk
  in the SpeedTree TLV walker. OPEN, unchanged.
- **#1843** (NIF-D1-01) — Pre-4.1 NIF bool fields read as 1 byte where the
  wire format is 32-bit on Morrowind-era NIFs. OPEN, unchanged (no target
  game currently exercises this version range).

These are all owned primarily by their respective per-game/NIF/NIFAL deep
audits; they are listed here only because dimension 6/7 of this audit's
checklist explicitly covers per-game translation-survey gaps and subsystem
coverage, and the dedup protocol requires confirming they're still open
and still accurate rather than silently dropping them from the compat
picture.

## Documented limitations re-confirmed (NOT findings — do not re-file)

- **FO4/FO76/Starfield ragdolls** — blocked on
  `BhkNPCollisionObject → BhkSystemBinary` decoder (PHYSAL §5). Unaffected
  by this window's ragdoll-leak fix.
- **`BhkPCollisionObject` phantoms** — parked pending a `TriggerVolume` ECS
  path (PHYSAL §5).
- **NIFAL parked passthroughs** — `bs_value_node`, `bs_ordered_node`,
  `tree_bones`, `range_kind`, `bs_lod_cutoffs`, `lod_group`, `bs_sub_index`,
  `NiSwitchNode`/`NiTextureEffect` (content-absent). No change this window.
- **`NiFogProperty`** — intentionally not dispatched (#1224); reads
  cell-scope `CellLighting`.
- **Emissive scale** — three `EmissiveSource` variants share ~1.0 scale; no
  normalization is correct (NIFAL §4).
- **Sun latitude** — no authored CLMT/WRLD latitude field exists;
  `SUN_SOUTH_TILT` is engine-defined (#1019 premise false; EXAL §9 Q1).
- **M42 AI-package v0 scope** — spawn-time-only selection, no pathing/NAVM,
  no animation-clip swap, ~10 of 17 procedures unimplemented, `PTD2`
  unparsed, `NearReference` resolution ceiling ~12% on real FNV data. All
  intentional, tracked scope (`docs/engine/npc-spawn-ai-packages.md`).
- **`PACK` `PSDT` era gap** — was a genuine gap (LC0716-01); now fixed
  (#2012/#2014/#2015, `55ae73e2`). Confirmed in place this pass — see
  boundary table above. Do not re-file.
- **Skyrim+ `PKDT` procedure-type byte** — verified compatible across eras
  (checked again this pass by reading `pack.rs`'s `PKDT` arm); not a
  finding.

---

## Summary

- **Total findings**: 0
- **CRITICAL**: 0
- **HIGH**: 0
- **MEDIUM**: 0
- **LOW**: 0

The compat surface stays clean at ca7a4e0e. All four canonical
single-producer boundaries (material / env / coord / ragdoll) re-verify
clean across a 106-commit window that was mostly renderer/FSR/decal/light
work rather than compat-surface work — and every touched file in or near
the boundaries (`material_translate.rs`, `env_translate.rs`, `ragdoll.rs`,
`crates/nif/src/import/collision/ragdoll.rs`, `crates/nif/src/import/material/mod.rs`,
`crates/nif/src/blocks/collision/constraints.rs`) was read in full rather
than trusted from its diff stat alone. The prior pass's one open finding
(`LC0716-01`, Skyrim+ `PACK.PSDT` schedule offset) is confirmed fixed and
tested. Ten pre-existing tracked issues (mostly Starfield-scoped NIFAL
passthroughs, owned by `/audit-starfield` and `/audit-nifal`) remain open
and unchanged; nothing new needs filing from this leg of the sweep.

---

*No `/audit-publish` step is needed — zero findings to file.*
