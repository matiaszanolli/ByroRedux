# Legacy Compatibility Audit — 2026-08-27

**Base:** `969d81c8` · **Type:** full `/audit-legacy-compat` sweep, all 7 dimensions,
run solo (no sub-agent fan-out) per explicit dispatch instruction, as part of a
`--preset comprehensive` audit-suite run.

## Scope

All seven dimensions: coordinate-system correctness (Z-up→Y-up), NIFAL
cross-layer mapping shape, the material translation boundary, PHYSAL's source
axis, EXAL/WATAL, per-game translation-survey patterns (A/B/C), and subsystem
coverage vs the legacy engines.

**Delta weighting.** 142 commits since the prior sweep
(`docs/audits/AUDIT_LEGACY_COMPAT_2026-08-24.md`, base `048a8bd8`) —
258 files, +16,676 / −2,372. The window is dominated by the FNV audit
close-out batch (`b78749af`, `6aa3d8f6`), the Skyrim SE / Starfield audit
close-outs, the #3321 FO3/FNV distant-object-LOD slice, and a large doc-rot
repair batch. Three of those touch this audit's boundaries directly
(`material_translate.rs` +237, `env_translate`/`object_lod`/`terrain_lod*`,
`import/collision/shape.rs` +154), so this sweep re-traced each claimed
single-producer contract from scratch rather than carrying the prior
report's conclusions.

**Source-availability statement.**

| Reference | Status |
|---|---|
| Gamebryo 2.3 source (`/media/matias/Respaldo 2TB/…/Gamebryo_2.3/`) | **UNMOUNTED** — `ls` returns "No such file or directory". Not consulted. Where a legacy-semantics question arose it was settled against the mounted alternatives below, and that substitution is stated at the finding. |
| `/mnt/data/src/reference/gamebryo-v32/Include/NiTexturingProperty.h` | **Consulted** — `ApplyMode` enum (`APPLY_REPLACE`/`APPLY_DECAL`/`APPLY_MODULATE`/`APPLY_DEPRECATED`/`APPLY_DEPRECATED2`, line 72-80). |
| `/mnt/data/src/reference/nifxml/nif.xml` | **Consulted** — `ApplyMode` enum (line 374-381), `TexturingFlags` bitfield (line 1582-1587), `NiTexturingProperty.Flags since="20.1.0.2"` (line 5232). |
| Vanilla mesh archives (Oblivion base + 7 DLC/SI, FO3, FNV, Skyrim SE Meshes0/1) | **Independently re-scanned this sweep** via two throwaway census probes (removed before finalisation; the tree is byte-identical to `969d81c8`). 55,949 NIFs / 642,589 `NiTransform` rotation matrices / 35,161 Oblivion `NiTexturingProperty` Apply Modes. Numbers below are measured, not inferred. |

**Method.** Every claimed single-producer boundary was re-traced with fresh
greps against HEAD. Two findings were settled by running code — one by a
temporary unit test against `repair_rotation_svd_or_identity`, one by two
temporary `crates/nif/examples/_tmp_lc_*.rs` probes over the vanilla archives.
Both instrumentations were reverted (`git checkout --`) and both probe files
deleted; `git status` on `crates/` and `byroredux/` is empty. Deduplicated
against 139 issues cached at `/tmp/audit/issues.json`, against `docs/audits/`
(including the 21 sibling reports from this same suite run), and against
`docs/engine/{nifal,exal,physal,watal,per-game-translation-survey}.md`. No
source file, game file, or GitHub issue was modified.

## Executive Summary

| Severity | Count |
|---|---:|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 1 |
| LOW | 4 |
| **Total** | **5** |

The three abstraction layers are structurally intact — NIFAL's material
boundary, EXAL's exterior boundary and PHYSAL's source axis all re-verified
single-producer, with no per-game branch downstream of any `translate()` and
no `bs_version >= N` literal outside tests. The prior sweep's one finding is
**verified fixed** (`a924244e`, #3281), with one residual passage.

The one MEDIUM is a genuine, measured content gap that no prior sweep has
touched: **Oblivion's only authored parallax signal is discarded inside the
NIF parser**, so no Oblivion mesh can reach the engine's parallax path at all.
It surfaced from Dimension 7's "flag any property whose authored effect is
dropped" bullet, which no recent sweep had actually exercised field-by-field.

The remaining four are LOW, and two of them are about this audit's own
reference material rather than the engine — the same class the last two
sweeps have yielded, which is itself a signal that the audit infrastructure's
doc currency is now the weakest link in this area.

### Per-dimension finding counts (every dimension enumerated)

| Dimension | CRIT | HIGH | MED | LOW | Findings |
|---|---:|---:|---:|---:|---|
| 1. Coordinate-system correctness (Z-up→Y-up) | 0 | 0 | 0 | 1 | LC-2026-08-27-D1-01 |
| 2. NIFAL — canonical NIF→ECS mapping shape | 0 | 0 | 0 | 0 | **none — clean** |
| 3. Material translation boundary | 0 | 0 | 0 | 0 | **none — clean** (one Existing: #3336) |
| 4. PHYSAL — per-game Havok → solver (source axis) | 0 | 0 | 0 | 0 | **none — clean** |
| 5. EXAL / WATAL — exterior + water → renderer & solver | 0 | 0 | 0 | 2 | LC-2026-08-27-D5-01, LC-2026-08-27-D5-02 |
| 6. Per-game translation-survey gaps (Pattern A/B/C) | 0 | 0 | 0 | 1 | LC-2026-08-27-D6-01 |
| 7. Subsystem coverage vs legacy | 0 | 0 | 1 | 0 | LC-2026-08-27-D7-01 |

---

## Dimension 1: Coordinate-system correctness (Z-up → Y-up)

**Findings: 1 (LOW).**

**Clean on both guarded axes.**

- **Single `(x, z, -y)` producer.** A fresh grep for the swizzle across
  `crates/` + `byroredux/` returns `crates/core/src/math/coord.rs:90`
  (`zup_to_yup_quat_wxyz`) as the only production site; every other hit is a
  doc comment on a documented delegation
  (`crates/nif/src/import/collision/mod.rs:540`,
  `crates/nif/src/import/mesh/tangent.rs:348`,
  `byroredux/src/cell_loader/placement_lod.rs:857`,
  `byroredux/src/systems/particle.rs:104`,
  `byroredux/src/cell_loader/terrain_lod.rs:985`) or a `#[cfg(test)]` pin. The
  new morph slice (`crates/nif/src/import/mesh/morph.rs`, #3231) routes its
  per-vertex deltas through `zup_point_to_yup` and pins the result
  (`morph.rs:168-169`) — it did not add a sixth swizzle.
- **No new bare `4096.0` cell math.** Every production hit resolves to
  `EXTERIOR_CELL_UNITS` (`crates/core/src/math/coord.rs:41`), to
  `RENDER_ORIGIN_SNAP`'s explicit re-export of it
  (`crates/renderer/src/vulkan/scene_buffer/constants.rs:378`), or to an
  unrelated quantity (`FOG_HEIGHT_REFERENCE_RAY_MAX_DISTANCE`,
  `LOCOMOTION_GROUND_RAY_MAX_DISTANCE`, `COMBUSTION_LIGHT_FIXED_SCALE`, a UV
  epsilon in `crates/physics/src/water.rs:351`). The two remaining literal
  `4096.0` cell-math sites are both inside `#[cfg(test)]`
  (`crates/core/src/ecs/components/camera.rs:259`,
  `byroredux/src/cell_loader/exterior.rs:1040`).
- **Winding.** `crates/nif/src/blocks/strip.rs:17-30` is the single de-strip
  implementation and swaps the **last two** indices on odd triangles
  (`(strip[i-2], strip[i], strip[i-1])`), matching the OpenGL/Vulkan CCW
  convention this dimension requires; the D3D first-two swap is explicitly
  ruled out in its module doc. `NiTriStripsData::to_triangles`
  (`crates/nif/src/blocks/tri_shape/ni_tri_shape.rs:601-611`) delegates.
- **REFR Euler.** `euler_zup_to_quat_yup_refr` remains the single dispatcher;
  no caller hardcodes a `--rotation-mode` or re-derives the ZYX product.

### LC-2026-08-27-D1-01: #2456's deferred-decision instrumentation now has its corpus answer (1 hit in 642,589 matrices), and its classifier cannot see the one case SVD cannot repair

- **Severity**: LOW
- **Dimension**: 1 — coordinate-system / transform-model fidelity (Dimension 7's "Transform model" bullet is the sibling)
- **Location**: `crates/nif/src/rotation.rs:52-60` (the warning cap + its stated purpose), `:105-162` (`repair_rotation_svd_or_identity` + `sanitize_rotation`), specifically `:117` (`if nearest.determinant() < 0.0`)
- **Status**: NEW
- **Description**: `sanitize_rotation` carries deliberately-temporary
  instrumentation whose own doc states its purpose:

  > "#2456 — this is diagnostic-only instrumentation to measure real corpus
  > incidence before committing to the larger 'decompose into
  > `NiTransform.scale`' fix; it changes no parsed geometry or transform
  > output." (`rotation.rs:55-57`)

  and

  > "Neither branch folds the discarded factor into `NiTransform.scale`
  > yet — that decomposition is deferred pending real-corpus incidence data."
  > (`rotation.rs:141-143`)

  That data now exists and it says the decomposition is not needed for any
  shipped Bethesda title. Two separate observations:

  1. **Measured incidence is effectively zero.** Instrumenting
     `sanitize_rotation` and running it over 55,949 vanilla NIFs — `Oblivion -
     Meshes.bsa`, `Fallout - Meshes.bsa` (FO3), `Fallout - Meshes.bsa` (FNV),
     `Skyrim - Meshes0.bsa` + `Meshes1.bsa` — yields **642,589**
     `NiTransform` rotation matrices, of which **1** trips
     `is_degenerate_rotation` (the SVD branch) and **0** trip the
     `is_non_orthonormal` pass-through branch. The `diag(2, 0.5, 1)`-shaped
     "baked scale/shear" case the deferred fix was designed for does not
     occur in any of the four corpora.
  2. **The classifier cannot distinguish a reflection from scale/shear, and
     silently changes orientation rather than losing magnitude.** A pure
     reflection (`diag(-1, 1, 1)`) is orthonormal — `is_non_orthonormal`
     returns `false` for it — but `is_degenerate_rotation` returns `true`
     (|det − 1| = 2), so it takes the SVD branch and logs the fixed text
     *"NiTransform.rotation is non-orthonormal (baked scale/shear,
     SVD-orthogonalized) — the singular value information is discarded"*
     (`rotation.rs:66-70`), which is factually wrong for it: a reflection has
     all singular values 1 and no scale/shear information to discard. Worse,
     `:117`'s `if nearest.determinant() < 0.0 { flip column 2 }` does not
     "repair" a reflection — it converts it into a **different orientation**.
     Verified by running the code: `diag(-1, 1, 1)` comes back as
     `diag(-1, 1, -1)`, i.e. a 180° rotation about Y, not an un-mirrored
     identity. So the eventual scale-decomposition fix would not address
     reflections at all, and the incidence data the warning gathers silently
     conflates the two classes.
- **Evidence**: Temporary counters added to `sanitize_rotation` and driven
  from a temporary `crates/nif/examples/_tmp_lc_rot.rs` (both reverted):
  `nifs=55949 matrices=642589 degenerate=1 nonortho_passthrough=0
  clean_reflection=0`. Separately, a temporary `#[test]` in
  `crates/nif/src/rotation.rs` printed
  `degenerate=true non_ortho=false maxcol=1` /
  `repaired=[[-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, -1.0]]` /
  `det_after=1` for `diag(-1, 1, 1)`.
- **Impact**: None on any shipped Bethesda title — this is why it is LOW, and
  why the reflection half is explicitly *not* filed as a content-mapping gap.
  Two costs remain: (a) a deferred design decision stays open with its
  blocking evidence already collectable in ten minutes, and every future
  audit that reads `rotation.rs:141-143` re-inherits "pending real-corpus
  incidence data" as an open question; (b) the reflection path is latent for
  non-Bethesda / mod content, which is live scope
  (issue #2383, "non-Bethesda titles"), and would fail in the most confusing
  possible way — a wrongly-*oriented* subtree reported in the log as a
  scale/shear problem.
- **Related**: #2456 (the instrumentation), #333 (the unit-quaternion guard
  downstream), #2383 (non-Bethesda titles). No existing issue covers the
  reflection classification.
- **Suggested Fix**: Record the measured incidence in `rotation.rs`'s doc (or
  close #2456 as "no vanilla incidence; decomposition not warranted") and
  drop or demote the rate-limited warning. If the instrumentation is kept,
  split the reflection case out: gate it on `det < 0` before the scale/shear
  wording, and say plainly in the message that the orientation — not just a
  magnitude — is being changed.

---

## Dimension 2: NIFAL — canonical NIF→ECS translation contract (mapping shape)

**Findings: 0.**

- **Downstream per-game-branch scan clean.** `grep -rn "GameKind"` over
  `crates/renderer/src`, `crates/physics/src`, `crates/core/src` returns the
  same four files as the prior sweep and no new ones: the shader-hygiene
  forbidden-token test string
  (`crates/renderer/src/vulkan/volumetrics.rs:3093,3097,3134`), two CHARAL
  doc comments (`crates/core/src/character/{mod,profile}.rs`) and one
  cross-reference doc comment (`crates/core/src/ecs/components/water.rs:35`).
  Zero code branches downstream of any `translate()`.
- **Pattern A clean.** `grep -rnE 'bs_version\s*(>=|<=|==|>|<)\s*[0-9]+'`
  over `crates` + `byroredux` returns **0** hits, test or otherwise — the
  named-constant migration is holding. The `user_version` comparisons that do
  remain are all inside `crates/nif/src/{header,version}.rs`, i.e. the
  version-decoding tier itself, which is where they belong.
- **New capture-only fields are documented as such.** The three fields added
  this window that do not yet reach a canonical sink all carry an explicit
  "captured, not consumed" rationale rather than being silent drops:
  `HkPackedNiTriStripsData.sub_parts` (#2550, `shape_mesh.rs`, per-sub-part
  Havok material for a future footstep/impact consumer),
  `ImportedMesh.bs_geometry_lod_slot` and `ImportedNode.lod_ranges`
  (`crates/nif/src/import/types.rs:200-212, 785-792`). Per this dimension's
  own framing these are *bounded known gaps*, not findings.
- **The `NiSwitchNode` / `NiLODNode` selection contract is honoured.**
  `crates/nif/src/import/walk/mod.rs:151-176` walks only the active child
  (switch) or child 0 (LOD), so no NIF authors all its LOD levels into the
  scene at once.
- **`NiFogProperty`** remains the one documented deliberate skip
  (`crates/nif/src/import/material/legacy_properties.rs:83-93`). Not
  re-filed.

---

## Dimension 3: Material translation boundary (NIFAL reference slice)

**Findings: 0 new.** One pre-existing gap re-confirmed and **not** re-filed.

- **`translate_material` is still the sole populated-`Material` producer.**
  Its three production callers are `byroredux/src/scene/nif_loader.rs`,
  `byroredux/src/cell_loader/spawn/mesh_instance.rs` and
  `byroredux/src/cell_loader/placement_lod.rs`; `byroredux/src/cornell.rs:1994`
  is the synthetic `--cornell` harness. The only literal `Material { … }`
  constructors outside `material_translate.rs` are `cornell.rs`'s
  self-contained scene builders and `#[cfg(test)]` fixtures.
- **`translate_texture_only_material`
  (`byroredux/src/material_translate.rs:671-704`) is not a second producer.**
  It owns no scalar literals — every canonical value comes from
  `Material::default()` or `resolve_pbr`'s classifier, the same one
  `translate_material` calls — and its three call sites plus
  `placement_lod`'s full-boundary call are source-scan-pinned by
  `every_exterior_spawner_inserts_a_boundary_material`
  (`material_translate.rs:1570-1605`).
- **The two-phase boundary is now declared** (#2330,
  `material_translate.rs:11-56`): `resolve_normal_alpha_spec_roughness` and
  `resolve_msn_z_source` are named as Phase-2 writers called from both spawn
  sites. Whether that doc's Skyrim claim is itself accurate is
  **#3370 (OPEN)**, filed by today's `/audit-skyrim` — out of scope here, not
  duplicated.
- **`attach_blend_and_facing_markers`
  (`material_translate.rs:589-638`, #2490) closed a real duplication**: the
  blend/decal/two-sided marker derivation had been byte-identical at both
  spawn sites. This is the marker-component counterpart of the `Material`
  consolidation and is the correct shape.

**Existing: #3336** — `byroredux/src/cell_loader/terrain_lod_btr.rs`'s
`spawn_btr_block` inserts `Transform`, `GlobalTransform`, `MeshHandle`,
`TextureHandle`, `MaterialTextureHandles`, `WorldBound`, `RenderLayer` and
`IsLodTerrain` (`:362-393`) but **no `Material`**, so the Skyrim/FO4 prebaked
`.btr` distant-terrain ladder — the *preferred* path
(`cell_loader/terrain_lod.rs:502-517` tries `.btr` first and only falls back
to heightmap synthesis) — still lands in
`byroredux/src/render/static_meshes.rs:354-367`'s hardcoded
`(0.5, 0.0, …)` no-`Material` arm, while its synth sibling
(`terrain_lod.rs:891-894`) goes through the boundary. Re-verified at HEAD;
already OPEN as #3336 from today's `/audit-fnv`-derived batch. Carried, not
re-filed. Note that `terrain_lod_btr.rs:133-137`'s own doc comment states the
consequence in the other direction (FO4 `_msn` terrain normals cannot be
routed because "LOD entities carry none").

---

## Dimension 4: PHYSAL — per-game Havok articulation → solver (source axis)

**Findings: 0.**

- **The seam is still only the constraint CInfo decode.**
  `grep -rn 'GameKind|game ==' crates/nif/src/import/collision/` returns zero
  hits; `extract_ragdoll` still switches on `BhkConstraintData` alone. The two
  typed decoders keep their `parse_oblivion` / `parse_fo3` arms confined to
  `crates/nif/src/blocks/collision/constraints.rs` (arms at `:86,121,167,197`,
  dispatched at `:302-303, 358-367, 412-414, 460-464`).
- **This window's two collision changes both narrowed, not widened, the
  seam.** `84dbf1bf` (#2550) replaced the FO3+ packed-mesh `stream.skip(12)`
  with a real `HkSubPartData` decode using the *same* three fields the
  Oblivion arm already read inline — byte count unchanged, so no caller's
  stream position moves — and captured the per-sub-part Havok material
  (measured: 5,634 packed meshes with a sub-part table, 1,780 multi-material,
  in `Fallout - Meshes.bsa`). `b78749af` (#3317) fixed
  `bhkCapsuleShape`/`bhkCylinderShape` placement by wrapping the primitive in
  the existing single-child `Compound` offset idiom
  (`crates/nif/src/import/collision/shape.rs:730-780`) rather than widening
  `CollisionShape` — and correctly returns the bare primitive when the
  segment really is origin-centred and Y-aligned, so nothing already correct
  changed shape.
- **FO4 / FO76 / Starfield ragdolls remain blocked on the
  `BhkNPCollisionObject → BhkSystemBinary` blob**, and the Havok cone+2-plane
  → Rapier per-axis limit mapping remains a documented approximation. Not
  re-filed.

---

## Dimension 5: EXAL / WATAL — per-game exterior environment → renderer & solver

**Findings: 2 (LOW).**

**Boundary shape re-verified.** `byroredux/src/env_translate.rs` remains the
sole construction site for `SkyParamsRes` (`:1068`, `:1286`) and
`WeatherDataRes` (`:1199`, `:1333`); every other hit in `byroredux/src` is a
`#[cfg(test)]` fixture (`commands_tests.rs:304`,
`scene/world_setup.rs:1213`, `systems/weather.rs:1509,1751,1767,1849`). The
procedural fallbacks are reached through one helper
(`scene/world_setup.rs:705-716`), called from the plugin-less path and from
`cornell.rs:1403-1404`. `resolve_water_material` and
`default_water_for_worldspace` keep their prior caller counts. No second
producer appeared.

**Per-game exterior logic is still funnelled through one `GameKind`-keyed
decision per quirk** — `terrain_lod_layout` (`env_translate.rs:61-64`) feeding
`combined_lod_supported` / `legacy_landscape_lod_supported`
(`byroredux/src/cell_loader/lod_support.rs:71,82`), plus the Oblivion water
default (`env_translate.rs:187`). #3321 added a third
(`ObjectLodScheme`) in the same shape, with a table test pinning that a game
declaring a scheme also has a band ladder. That is the doctrine working.

### LC-2026-08-27-D5-01: this skill's own Dimension 5 text asserts two gaps that #3321 and the VWD consumer wiring have since closed

- **Severity**: LOW
- **Dimension**: 5 — EXAL, audit-infrastructure currency
- **Location**: `.claude/commands/audit-legacy-compat/SKILL.md`, Dimension 5's "LOD — status" bullet
- **Status**: NEW
- **Description**: Two of that bullet's load-bearing claims are false at HEAD.
  1. *"FO3/FNV ship **zero** `distantlod\*.lod` files in any vanilla archive
     (FO3-D4-01 / #2086) — 'FO3/FNV distant-object LOD is missing' is a real
     gap but not a `placement_lod.rs` gap."* The first half still holds and
     `placement_lod_supported` is still Oblivion-only
     (`byroredux/src/cell_loader/placement_lod.rs:313`, pinned at `:754-760`).
     The second half is now wrong: `e23a9908` (#3321, 2026-08-27) established
     that FO3/FNV ship a **third** scheme —
     `meshes\landscape\lod\<world>\blocks\<world>.level<L>.x<qx>.y<qy>.nif`,
     structurally the `.bto` shape in a different container — and consumed it
     as an `ObjectLodScheme` arm inside the existing
     `byroredux/src/cell_loader/object_lod.rs` (module doc rewritten at
     `:18-37` to say so explicitly). The commit records a live verification of
     280 quads loaded on `WastelandNV (0,0)` where the pre-fix engine loaded
     0. FO3/FNV distant-object LOD is no longer a gap.
  2. *"The **VWD / 'Has Distant LOD' record-header flag** is now parsed and
     exposed (`RecordHeader::is_visible_when_distant()`, #1731) but **has zero
     consumers**."* It has consumers: the flag is captured onto placements
     (`crates/plugin/src/esm/cell/support.rs:361,599,663,728`), stamped as an
     ECS row by `stamp_visible_when_distant`
     (`byroredux/src/cell_loader/references/synth_child.rs:682,744-750`), and
     read by the LOD reconcile loop (`resident_vwd_refr_cells`, cited by
     **#3142 OPEN**). What remains open is *full-model culling* from it, which
     is tracked as **#3307 OPEN** ("EX-10/11 item 8: active VWD full-model
     culling") — a narrower and differently-owned gap than "zero consumers".
- **Evidence**: `e23a9908`'s diff and commit body; the greps cited above; the
  corrected `docs/engine/exal.md` §5 (the commit rewrote it in the same
  change, so the *doc* is current and only the *skill* lagged).
- **Impact**: Documentation-only, but self-inflicted on the audit pipeline:
  Dimension 5 tells the auditor these are real coverage gaps ("Findings here
  are real coverage gaps, not premise errors"), so the next sweep is being
  actively steered toward re-filing two closed items. This is the third
  consecutive legacy-compat sweep whose yield includes stale audit reference
  material.
- **Related**: LC-D6-2026-08-24-01 and LC-D6-03 (2026-08-20) — same class,
  different documents. #3321 (closed), #3307 (open), #3142 (open).
- **Suggested Fix**: Replace the FO3/FNV clause with the post-#3321 state
  (three schemes: Oblivion placement lists → `placement_lod.rs`; FO3/FNV
  `blocks\` quads and Skyrim/FO4 `.bto` → `object_lod.rs`), and replace "zero
  consumers" with "consumed for LOD residency; full-model culling is #3307".
  `docs/engine/exal.md` §5 is already correct and can be quoted directly.

### LC-2026-08-27-D5-02: `assemble_exterior_streaming` carries an undocumented `game == Skyrim` branch with two hardcoded vanilla FormIDs

- **Severity**: LOW
- **Dimension**: 5 — EXAL boundary shape ("scattered new `if game == …`
  exterior logic is a finding")
- **Location**: `byroredux/src/scene/world_setup.rs:933-941`
- **Status**: NEW
- **Description**: The shared exterior-streaming assembly ends with:

  ```rust
  if state.wctx.record_index.game == byroredux_plugin::esm::reader::GameKind::Skyrim {
      crate::asset_provider::materialize_scene_actor_alias_stubs(
          world,
          &state.wctx.record_index,
          &state.wctx.load_order,
          0x0003_372B,
          0x000B_ECD4,
      );
  }
  ```

  with **no comment at the call site**. `materialize_scene_actor_alias_stubs`
  (`byroredux/src/asset_provider/script.rs:565-577`) is itself a properly
  general, well-documented helper parameterised on quest + scene FormID, and
  this is its **only** caller. So the whole per-title specificity — the
  `GameKind` gate and the two literal Skyrim MQ101 form IDs — sits in
  `assemble_exterior_streaming`, which is the common entry point for four
  callers (boot's `--grid` mode, `App::step_cell_transition`'s Exterior arm,
  the `dbgload` exterior command, and `save_io`'s reload path — enumerated in
  `begin_exterior_streaming`'s own doc at `:947-963`).
- **Evidence**: The snippet above, verbatim from HEAD;
  `grep -rn materialize_scene_actor_alias_stubs byroredux/src` returns exactly
  the definition and this one call.
- **Impact**: No runtime impact today — it is gated, and the M47.2 MQ101 slice
  is a deliberately scoped demo. The cost is shape: this is the seed of the
  per-title-content-hack pattern in a game-agnostic path. A second scoped
  scene (any game) has nowhere to go but a second arm here, and there is no
  in-code pointer to `docs/engine/m47-2-design.md` telling a reader that the
  scope is intentional. It also silently means no non-Skyrim title gets forced
  quest-alias stubs even where its content needs them. Related but distinct
  from **#2664 (CLOSED)**, which fixed this same code's *open-coded stamper*
  and left the gate untouched.
- **Related**: #2664 (closed), `docs/engine/m47-2-design.md`.
- **Suggested Fix**: Either move the (game, quest, scene) triple into a small
  table beside the other per-game exterior decisions (the
  `terrain_lod_layout` / `ObjectLodScheme` shape), or leave it in place with a
  three-line comment naming MQ101, pointing at `m47-2-design.md`, and stating
  the intended scope — so the next auditor does not have to reverse-engineer
  two hex literals to decide whether it is a leak.

**WATAL.** Both water producers still share `water_kind_from_name`
(`byroredux/src/material_translate.rs:226`); `grep 'WaterMaterial {'`
finds only `env_translate::resolve_water_material` and
`material_translate::attach_mesh_water` outside tests. `#3270 (OPEN)` — FO4
`WATR.DNAM` fog near/far offsets — is `/audit-esm`'s and is not duplicated
here.

---

## Dimension 6: Per-game translation-survey gaps (Pattern A/B/C)

**Findings: 1 (LOW).**

Patterns A and B re-checked clean (see Dimension 2). Pattern C (variant-enum
struct shapes) picked up a correct new instance this window in
`ObjectLodScheme`.

### LC-D6-2026-08-24-01 (prior sweep) — verified FIXED, with one residual

The prior sweep's only finding was that
`docs/engine/per-game-translation-survey.md` was three months stale and its
§2 headline example plus §4.3 `RACE DATA` bullet described bugs that had been
fixed. Both were corrected by `a924244e` (2026-08-25, #3281):

- The `Status:` line now reads *"generated 2026-05-28 … hand-corrected
  2026-08-25 (#3281 — two stale passages fixed, see notes inline at §2 and
  §4.3)"* (`per-game-translation-survey.md:3-5`), and a retained-as-evidence
  banner plus an explicit "the `~70+ per-game branches` headline count is
  **unverified against the current tree**" caveat were added (`:9-16`).
- §2 now opens *"This section's original worked example is stale — see the
  note below"* and states that #1873 / `634873db` fixed it (`:53-64`).
- §4.3's RACE bullet now reads *"**fixed since this survey was written.** A
  dedicated Skyrim arm (`records/actor/mod.rs:1225`, gated on
  `GameKind::Skyrim` + `len` of 128 or 164) now exists"* (`:231-235`).

Not re-filed. The residual is below.

### LC-2026-08-27-D6-01: the survey's §7 still states, unmarked, the exact `classify_pbr_keyword` claim §2 now retracts

- **Severity**: LOW
- **Dimension**: 6 — per-game translation survey currency
- **Location**: `docs/engine/per-game-translation-survey.md:427-430` (§7 item 7), contradicting `:53-64` (§2's correction note) in the same file
- **Status**: NEW (residual of LC-D6-2026-08-24-01)
- **Description**: The 2026-08-25 correction pass fixed §2 and §4.3 but left
  §7's numbered list untouched. Item 7 still reads:

  > "7. **FNV `classify_pbr_keyword` collapses everything to matte 0.8
  > roughness** — already documented in `material-abstraction.md` Leak B. This
  > single fact accounts for the 'Fallout looks like a different engine'
  > perception more than any other."

  §2 of the same document now says the opposite 370 lines earlier — that this
  "specific bug was fixed by #1873 (commit `634873db`)" and that the
  classifier "now runs an evidence-cited keyword + `specular_authored` gate
  rather than a blanket matte default". §7 carries no correction marker of any
  kind.
- **Evidence**: Both passages quoted above are verbatim from HEAD.
  `crates/core/src/ecs/components/material.rs:663+` is the current classifier;
  `docs/engine/material-abstraction.md:10-13` carries its own corrective
  banner for the same fix, so §7's cross-reference to "Leak B" now points at a
  passage that is itself annotated as superseded.
- **Impact**: Documentation-only, but §7 is precisely the section this
  skill's Dimension 6 names by number — *"**Fallout is the stress case**
  (§7)"*. An auditor following that pointer reads the uncorrected version of
  the exact claim the correction pass was run to remove, and the correction is
  invisible unless they also read §2.
- **Related**: LC-D6-2026-08-24-01 (2026-08-24), #3281 (the partial fix),
  #1873 (the underlying closed bug).
- **Suggested Fix**: Delete §7 item 7 or annotate it in place the way §2 and
  §4.3 now are, and re-scan §7's other six items for the same residue while
  the file is open — the correction pass fixed the two passages the prior
  audit cited by line number, not the claims wherever they appeared.

**Carried, not re-filed: LC-D6-02 / #3146 (OPEN).** `decode_data`'s 144–220
tail in `crates/plugin/src/esm/records/misc/water.rs` remains unreachable on
every vanilla record. Structurally unchanged this window.

---

## Dimension 7: Subsystem coverage vs legacy

**Findings: 1 (MEDIUM).**

**Improvements this window, none of them gaps.** The `NiGeomMorpherController`
/ `NiMorphData` slice (#3231) closed one of this dimension's two named parked
channels: morph-target vertex deltas are now extracted
(`crates/nif/src/import/mesh/morph.rs`), carried on
`ImportedMesh.morph_targets`, uploaded per entity as a `MorphSlot`
(`byroredux/src/cell_loader/spawn/mesh_instance.rs:724-742`) and driven by a
real animation channel (`FloatTarget::MorphWeight`,
`crates/nif/src/anim/sequence.rs:82-89`). `#3316` likewise closed a large
animation gap: a `NiTransformInterpolator` with a null `data_ref` is now
treated as a constant pose rather than "no channel"
(`crates/nif/src/anim/transform.rs:46-67`), recovering 49,182 of 123,729 FNV
controlled blocks.

**Property → pipeline mapping, walked field-by-field.** All nine parsed
property types reach a landing site in
`crates/nif/src/import/material/legacy_properties.rs`:
`NiAlphaProperty` (`:98-114`), `NiZBufferProperty` (`:116-129`),
`NiMaterialProperty` (`:131-161`), `NiTexturingProperty` (`:163-…`),
`NiStencilProperty` (`:675-697`), `NiVertexColorProperty` (`:738-…`), and the
four `NiFlagProperty` subtypes — `NiSpecularProperty`, `NiWireframeProperty`,
`NiShadeProperty`, `NiDitherProperty` (`:699-735`). `NiFogProperty` is the one
documented skip. The **field-level** walk is where the finding below came
from.

**Other checks clean.** Bone-name → entity resolution is case-insensitive at
both PHYSAL binding sites (`byroredux/src/ragdoll.rs:101,105`, via
`crate::name_lookup::get_case_insensitive`, #2458), so the `NiFixedString`
→ `StringPool` interning difference is not load-bearing. `compose_transforms`
(`crates/nif/src/import/transform.rs:13-25`) matches Gamebryo's
`NiTransform` composition exactly (rotation product, parent-scaled child
translation, scale product). Billboard mode reaches the ECS on both walks —
per-node for the loose-NIF path (`byroredux/src/scene/nif_loader.rs:547`) and
per-mesh for the flat cell-loader path
(`byroredux/src/cell_loader/spawn/mesh_instance.rs:792`, #2206).

### LC-2026-08-27-D7-01: Oblivion's only authored parallax signal — `NiTexturingProperty` Apply Mode `APPLY_HILIGHT2` — is read and discarded in the parser, and no other Oblivion parallax path exists

- **Severity**: MEDIUM
- **Dimension**: 7 — property → pipeline mapping ("flag any property whose authored effect is dropped")
- **Location**: `crates/nif/src/blocks/properties.rs:205-209` (the discard), `:117` (`NiTexturingProperty.flags`, zero consumers), `crates/nif/src/import/material/legacy_properties.rs:210-238` + `crates/nif/src/blocks/properties.rs:262` (the version gate that makes the only implemented parallax path unreachable on Oblivion)
- **Status**: NEW
- **Description**: `NiTexturingProperty::parse` reads the Apply Mode field and
  throws it away without storing it, without a landing site, and — unlike
  `NiFogProperty` — **without any comment recording that the drop is
  deliberate**:

  ```rust
  // Apply Mode: since 3.3.0.13, until 20.1.0.1.
  // `until=` is inclusive per the version.rs doctrine — present at v20.1.0.1.
  if stream.version() <= NifVersion::STRING_TABLE_THRESHOLD {
      let _apply_mode = stream.read_u32_le()?;
  }
  ```

  For NIF ≥ 20.1.0.2 the same field moves into the `Flags` bitfield
  (nif.xml:1585 — `TexturingFlags`, `width="3" pos="1" mask="0x000E"
  name="Apply Mode" type="ApplyMode"`; nif.xml:5232 gates `Flags` at
  `since="20.1.0.2"`). Redux stores that word
  (`properties.rs:117`, `pub flags: u16`) but **nothing in the workspace ever
  reads `NiTexturingProperty.flags`** — a grep for `tex_prop.flags` returns
  zero hits; the only `.flags` reads in the legacy walker are `TexDesc.flags`
  for clamp mode (`legacy_properties.rs:274`).

  That drop is harmless on every post-Oblivion title (measured below), but on
  Oblivion it removes the game's only parallax signal. nif.xml annotates the
  value directly:

  > `<option value="4" name="APPLY_HILIGHT2">Parallax Flag in some Oblivion
  > meshes.</option>` (nif.xml:380)

  and Gamebryo v3.2 confirms the value has no surviving general meaning —
  `/mnt/data/src/reference/gamebryo-v32/Include/NiTexturingProperty.h:72-80`
  renames modes 3 and 4 to `APPLY_DEPRECATED` / `APPLY_DEPRECATED2`.

  The engine's three `parallax_map` producers are all **version-gated above
  Oblivion's v20.0.0.5**, so none can fire for it:

  | Producer | Gate |
  |---|---|
  | `NiTexturingProperty` slot 7 (`legacy_properties.rs:215-217`) | `is_v20_2_0_5_plus` (`properties.rs:262`) — the slot does not exist on Oblivion |
  | `BSShaderTextureSet` slot 3 (`legacy_properties.rs:339-342`) | `BSShaderPPLightingProperty`, FO3+ |
  | `TextureRole::Height` (`dedicated_shader.rs:207`) | BGSM/BGEM, FO4+ |

  So Oblivion parallax is currently 0% mapped, and the file-level flag that
  would enable it is destroyed one line after it is read.
- **Evidence**: Measured with a temporary counter on the discard site, driven
  over the vanilla archives (probe removed; tree unmodified).
  - `Oblivion - Meshes.bsa` + all 7 DLC/SI mesh archives — 9,537 NIFs, all
    parsed: **35,161** `NiTexturingProperty` Apply Modes —
    0 `APPLY_REPLACE`, 18 `APPLY_DECAL`, 32,810 `APPLY_MODULATE` (the
    no-op default), 900 `APPLY_HILIGHT`, **1,433 `APPLY_HILIGHT2`**.
  - Base + Shivering Isles alone: **741 distinct NIFs of 9,470** carry at
    least one `APPLY_HILIGHT2`. Sampled paths are exactly the content class
    the convention is known for — `meshes\dungeons\caves\crmcornerinside01a.nif`,
    `meshes\dungeons\caves\crmfloorcrevice01a.nif`,
    `meshes\architecture\stonewall\stonewallbend02lm.nif`,
    `meshes\rocks\greatforest\lichen\rockgreatforest2080fgllichen.nif`.
  - The post-Oblivion generation is unaffected: reading the Apply Mode bits
    out of `NiTexturingProperty.flags` over `Fallout - Meshes.bsa` (FNV,
    14,881 NIFs), `Fallout - Meshes.bsa` (FO3) and
    `Skyrim - Meshes0.bsa` (29,851 NIFs combined) gives **2,258 + 996
    properties, 100% `APPLY_MODULATE`, 0 NIFs with a non-default mode**.
    The gap is Oblivion-only and bounded.
  - Height source: `Oblivion - Textures - Compressed.bsa` contains **zero**
    `_p.dds` entries, so Oblivion does not ship a separate parallax/height
    texture — consistent with the normal-map-alpha convention, for which the
    engine already has the analogous machinery on the Skyrim spec side
    (`NORMAL_ALPHA_SPEC_BIT`, `material_translate.rs:719`).
- **Impact**: Every parallax-authored Oblivion surface renders flat — 741
  vanilla meshes, concentrated in cave and stone architecture and rock
  clutter, i.e. the interior/exterior surfaces a player looks at most. It is
  not a crash or a content loss (geometry and normal maps still render), which
  is why this is MEDIUM rather than HIGH under the "escalate if it removes
  visible game content" rule. The second-order cost is that the drop is
  invisible: with no field on the struct and no comment, nothing in the tree
  records that Oblivion parallax authoring was ever seen and declined, so this
  gap is not discoverable from the code.
- **Related**: No existing issue mentions `apply_mode`, `APPLY_HILIGHT`, or
  Oblivion parallax (139 issues scanned); no prior report in `docs/audits/`
  mentions them either. Adjacent but distinct: **#3073 (OPEN)** —
  `parallax_height_scale` / `parallax_max_passes` bypassing the canonical
  `Material` — which is about the scalars, not about whether parallax is
  detected at all. Sibling convention already implemented:
  `resolve_normal_alpha_spec_roughness` (#1480).
- **Suggested Fix**: Two steps, separable. (1) Stop discarding the field:
  store `apply_mode: u32` on `NiTexturingProperty` (decoding it from
  `(flags >> 1) & 0x7` on the ≥ 20.1.0.2 branch so both generations land in
  one place), which alone makes the authored value visible to a future
  consumer and to `mat.dump`. (2) Route `APPLY_HILIGHT2` into
  `MaterialInfo.parallax_map` on the Oblivion branch, sourcing height from the
  normal map's alpha via the same "bit tells the shader to sample another
  slot's alpha" mechanism `NORMAL_ALPHA_SPEC_BIT` already uses — do **not**
  add a `_p.dds` path-synthesis fallback, since the census shows no such
  texture ships. If step 2 is deferred, step 1 plus a one-line
  deliberate-skip comment in the `NiFogProperty` style is the minimum, so the
  next auditor does not have to re-derive this from nif.xml.

---

## Deduplication

`/tmp/audit/issues.json` (139 issues, fetched at HEAD) was keyword-scanned for
every candidate: `parallax|apply mode|apply_mode|texturing`,
`btr|lod terrain|distant terrain|terrain lod|material|2444`,
`alias|mq101|scene`, `coordinate|euler|4096|axis|rotation|determinant`,
`ragdoll|havok|constraint|subpart`, `water|watr|watal`,
`translation-survey|classify_pbr`. `docs/audits/` was scanned for prior
write-ups, **including the 21 sibling reports produced by this same
2026-08-27 suite run** (`AUDIT_NIFAL_2026-08-27.md` and
`AUDIT_SKYRIM_2026-08-27.md` both discuss parallax — checked, both are about
`#3073`'s scalar bypass and the Phase-2 doc claim respectively, neither about
Apply Mode).

| Finding | Nearest existing | Verdict |
|---|---|---|
| LC-2026-08-27-D7-01 (Oblivion Apply Mode / parallax) | none — `apply_mode`/`APPLY_HILIGHT` appear in no issue and no prior audit; #3073 is the adjacent-but-different scalar bypass | **NEW** |
| LC-2026-08-27-D1-01 (#2456 instrumentation + reflections) | #2456 is the instrumentation itself, not a filed finding about it | **NEW** |
| LC-2026-08-27-D5-01 (skill Dim-5 staleness) | LC-D6-03 / LC-D6-2026-08-24-01 are the same class, different files | **NEW** |
| LC-2026-08-27-D5-02 (MQ101 gate in `assemble_exterior_streaming`) | #2664 (CLOSED) touched this code's stamper, not the gate | **NEW** |
| LC-2026-08-27-D6-01 (survey §7 residual) | LC-D6-2026-08-24-01 (fixed via #3281) — this is its residual | **NEW** |
| `.btr` spawns no `Material` | **#3336 OPEN** | Existing — re-verified at HEAD, not re-filed |
| `decode_data` 144–220 tail | **#3146 OPEN** (= LC-D6-02) | Carried, not re-filed |
| `material_translate.rs` Phase-2 doc claim | **#3370 OPEN** (`/audit-skyrim`, today) | Owned elsewhere, not duplicated |
| FO4 `WATR.DNAM` fog offsets | **#3270 OPEN** (`/audit-esm`) | Owned elsewhere, not duplicated |
| VWD full-model culling | **#3307 OPEN** | Owned elsewhere, cited as evidence only |

Skipped as already OPEN and owned elsewhere (not duplicated here): the NIFAL
per-slice backlog (#2423, #2490-adjacent, #2532, #2533, #2571, #2697, #3072–
#3075, #3187), the FNV close-out batch (#3327–#3354), the Skyrim batch
(#3364–#3371), and the Starfield batch (#3396, #3398) — all per-slice or
per-game contents, outside this audit's cross-layer mapping-shape scope.

## Verification

Read-only source review of the tree at `969d81c8`, cross-referenced against
the prior sweep's report and against `git log` / `git show` for the 142
commits since `048a8bd8`. Two temporary instrumentations were applied and
reverted:

- `crates/nif/src/rotation.rs` — a 4-slot atomic counter in
  `sanitize_rotation`, driven by `crates/nif/examples/_tmp_lc_rot.rs`, plus a
  throwaway `#[test]` printing `repair_rotation_svd_or_identity(diag(-1,1,1))`.
- `crates/nif/src/blocks/properties.rs` — an 8-slot atomic histogram at the
  `apply_mode` discard, driven by `crates/nif/examples/_tmp_lc_applymode.rs`;
  plus a probe-only `crates/nif/examples/_tmp_lc_texflags.rs` that needed no
  source patch (it reads the already-public `NiTexturingProperty.flags`).

All three example files were deleted and both source files restored with
`git checkout --`; `git status --short crates/ byroredux/` is empty. The
Gamebryo 2.3 source drive is **unmounted**, so no claim below rests on it —
the two legacy-semantics questions that arose were settled against
`/mnt/data/src/reference/gamebryo-v32/Include/NiTexturingProperty.h` and
`/mnt/data/src/reference/nifxml/nif.xml`, both cited inline. `cargo check -p
byroredux-nif --tests` was run (clean, 3.68s); no full-workspace build or test
run was attempted, since no finding here turns on a workspace-wide
compile-time question. No source file, game file, or GitHub issue was
modified.

## Summary

- **Findings:** 5 (all NEW) — 0 CRITICAL, 0 HIGH, 1 MEDIUM, 4 LOW.
- **Prior sweep:** its single finding (LC-D6-2026-08-24-01) is verified fixed
  at HEAD by `a924244e` / #3281, with one uncorrected residual passage filed
  above as LC-2026-08-27-D6-01. No regressions anywhere in scope.
- **Boundary health:** NIFAL / EXAL / PHYSAL / WATAL all structurally intact
  across 142 commits. The two changes that touched collision this window
  (#2550, #3317) both *narrowed* the per-game seam rather than widening it,
  and #3321's `ObjectLodScheme` is a textbook Pattern-C variant enum. Pattern
  A is at **zero** hits workspace-wide.
- **The real result** is Dimension 7's, and it came from walking
  `NiTexturingProperty` field-by-field rather than type-by-type: Oblivion's
  parallax flag is destroyed one line after it is parsed, and because all
  three implemented parallax producers are version-gated above Oblivion, the
  game has no parallax at all. 1,433 authored properties across 741 vanilla
  meshes, measured. The equivalent field-level walk has not been done for the
  other eight legacy properties — `NiTexturingProperty`'s bump-map
  `luma_scale` / `luma_offset` / 2×2 matrix are dropped the same way
  (`properties.rs:248-256`) and were not chased here.
- **Where the rest of the yield lives:** three of the four LOW findings are
  documentation, and two of those are in the audit infrastructure itself —
  this skill's Dimension 5 text and the survey document Dimension 6 points at.
  That is now three consecutive sweeps with the same shape, and it argues for
  re-validating the skill's own prose against HEAD as part of the sweep rather
  than trusting it.
- **Highest-value fix:** store `NiTexturingProperty`'s Apply Mode instead of
  discarding it (LC-2026-08-27-D7-01 step 1) — it is a two-line change that
  makes an invisible whole-game gap visible, independent of when the parallax
  routing itself lands.

Suggested next step:
```
/audit-publish docs/audits/AUDIT_LEGACY_COMPAT_2026-08-27.md
```

TALLY: CRITICAL=0 HIGH=0 MEDIUM=1 LOW=4
