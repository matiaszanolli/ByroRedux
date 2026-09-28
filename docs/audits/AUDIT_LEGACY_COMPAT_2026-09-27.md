**HEAD**: `ee952b499` · **Baseline**: `docs/audits/AUDIT_LEGACY_COMPAT_2026-09-22.md` (HEAD `ee6d3fb39`) · **Audited**: Dims 1, 2, 3 (all three had commits since the baseline on their listed paths) · **Unchanged since baseline (skimmed)**: none at dimension level; the sub-areas with zero commits are listed at the end

# Legacy Compatibility Audit — 2026-09-27

## Method

Delta-scoped against the 2026-09-22 baseline (230 commits in range). For each
dimension I re-ran the `First step:` commands fresh against HEAD, read every
commit in range that touched the dimension's `Paths:`, and traced changed
boundaries to their callers. Findings were deduped against a fresh
`gh issue list --state all` (1,000 most recent) plus `docs/audits/`. The
Gamebryo 2.3 drive is **unmounted**, so legacy cross-checks used
`/mnt/data/src/reference/gamebryo-v32/Include/` and `nifxml/nif.xml`. No
source, test or issue was modified, and no engine binary was launched.

## Executive Summary

| Severity | Count (NEW + regression) |
|---|---:|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 1 |
| LOW | 1 |
| **Total** | **2** |

Still open and matched to existing issues (re-verified, not re-filed):
`#4126` (COORD-01), `#4764` (COORD-03) and `#4128` (SUBSYS-01).
**`#4127` (COORD-02) is close-eligible**: `docs/engine/coordinate-system.md`
now describes `xcll_direction_yup` correctly, which is exactly what the issue
asked for (verified against `byroredux/src/cell_loader/load.rs:219,253`).

**Headline.** The one legacy-subsystem change in range is correct:
`ab255cfd2` changed the NIF `NiSpotLight` import to degrees→radians. It
matches Gamebryo, which computes `NiCos((fSpotAngle * NI_PI) / 180.f)` in
`NiSpotLight.inl:26`, and nif.xml's `range="#F_DEG#"`. That fix
(`#4859`, closed) is verified here, not re-filed.

The MEDIUM is a different light-translation divergence at the adjacent
boundary. One ESM `LIGH` record resolves to two different canonical falloff
exponents on FO3/FNV/Oblivion, depending only on whether the lamp's NIF
carries a spawnable `NiLight`. The `#4514` guard classifies this spawn site
as out of scope on a premise that doesn't hold.

---

## Dimension 1 — Coordinate + placement fidelity (Z-up → Y-up)

Commits since baseline on the listed paths: `c14f5361a` (2026-09-24, #4056).
It adds `LAND_TEXTURE_TILES_PER_CELL` to `crates/core/src/math/coord.rs` as
the single source for terrain UV tiling. I verified it is consumed everywhere
it is needed (`cell_loader/terrain.rs:48,912-913`, `terrain_lod.rs:672-674`,
`render/groundcover.rs:1459`) with no leftover literal. That is a clean
consolidation, not a regression.

**Re-verified clean:**
- **Swap SoT**: none of the `(x, z, -y)` grep hits in `byroredux/src` or
  `crates` is an inline re-implementation; all are comments or docstrings.
  The only two production `Quat::from_mat3` sites
  (`crates/nif/src/import/collision/mod.rs:731`,
  `crates/physics/src/ragdoll.rs:727`) predate the baseline and carry their
  documented exceptions (a Havok column-major basis change; a ragdoll frame
  built from basis vectors).
- **Strip de-stitch**: one implementation, `strip::destrip`, called from
  `skin.rs:335`, `ni_tri_shape.rs:628` and `collision/shape.rs:662`. No copy.
- **REFR dispatcher**: `REFR_ROTATION_MODE` still defaults to `1`
  (`cell_loader/euler.rs:34`). The `mode.min(3)` clamp and the stale
  "Defaults to 0" comment (`boot/mod.rs:298,301`) are both still present and
  covered by `#4126`. The contradicting doc paragraph is still `#4764`.
- **Cell grid**: every production `4096.0` hit outside `coord.rs` is a
  distance, a UV clamp or a test fixture, with one exception, filed below as
  COORD-04.

### COORD-04: `crates/bsa/src/uvd.rs` declares a second `EXTERIOR_CELL_UNITS`, not pinned to the coord SoT
- **Severity**: LOW
- **Dimension**: 1 — Coordinate + placement fidelity (exterior grid single source)
- **Location**: `crates/bsa/src/uvd.rs:195` (used at `:217`, `:227`, `:234`)
- **Status**: NEW (landed `00b75ed00`, 2026-09-15; the 2026-09-22 run did not
  flag it)
- **Description**: The skill names `EXTERIOR_CELL_UNITS` in
  `crates/core/src/math/coord.rs` as the sole source for cell-grid math
  (#1112 collapsed six divergent literals, one of which had a sign bug). FO4
  previs grid recovery, `UvdHeader::exterior_cell_grid`, declares its own
  `pub const EXTERIOR_CELL_UNITS: f32 = 4096.0` with the same name and does
  grid math with it (`(min / EXTERIOR_CELL_UNITS).floor() as i32 + 1`). The
  `byroredux-bsa` crate has no `byroredux-core` dependency, so it can't
  import the SoT. That is a reasonable isolation choice, but nothing pins
  the two constants equal. The module is private (`mod uvd;`,
  `lib.rs:35`), so the constant isn't re-exported, and a pin can't be written
  from outside without a re-export.
- **Evidence**:
  ```rust
  // crates/bsa/src/uvd.rs:193-195
  /// Side of one Fallout 4 exterior cell, in game units. An exterior `.uvd`'s
  /// bounds are a whole number of these on X and Y.
  pub const EXTERIOR_CELL_UNITS: f32 = 4096.0;
  ```
- **Impact**: None today. There's no production consumer: the only caller of
  `exterior_cell_grid` is `crates/bsa/examples/probe_uvd_corpus.rs`, and
  `cell_loader/precombined.rs:52-72` only cites it in docs. The value is a
  format fact that won't change. The risk is structural: a second
  same-named source for the exact quantity #1112 consolidated. It becomes
  load-bearing once `precombined.rs` wires previs-grid recovery into
  placement.
- **Related**: #1112. Incidental and not filed: `crates/core/src/ecs/components/camera.rs:1156`'s
  test `far_plane_clears_the_widest_lod_ring_corner` declares a local
  `const CELL_UNITS: f32 = 4096.0` inside the crate that owns the SoT. It is
  test-only, but its doc says it exists "so the number cannot drift", and the
  local literal defeats that.
- **Suggested Fix**: Re-export the constant (`pub use uvd::EXTERIOR_CELL_UNITS
  as UVD_CELL_UNITS`) and add a
  `const _: () = assert!(byroredux_bsa::UVD_CELL_UNITS == coord::EXTERIOR_CELL_UNITS);`
  in `byroredux`, which depends on both crates. Alternatively, rename it so
  the grep for the SoT name has one hit.

---

## Dimension 2 — Legacy subsystem coverage

Commit volume in range is heavy (~25 on the listed paths). Most of it is the
NIFAL and material-boundary charter: #4279 effect-shader `Own_Emit`, #4426
flipbook roles, #4560 emitter rate, #4282/#4283 NIFAL sinks, #4430 FO4 glow
census. Those were re-homed to `/audit-nifal`, so they are not re-derived here.
I read the commits that fall under this dimension's own charter:

- **`ab255cfd2` — `NiSpotLight` angle units (light subsystem).** Verified
  correct against the legacy source. `gamebryo-v32/Include/NiSpotLight.inl:26`
  stores the authored value and derives the cone cosine as
  `NiCos((fSpotAngle * NI_PI) / 180.f)`: degrees, used as a half-angle.
  nif.xml `NiSpotLight` declares `range="#F_DEG#"`. The walker now passes
  `outer_spot_angle.to_radians()` (`import/walk/lights.rs:91`) into
  `ImportedLight.outer_angle` (half-angle radians), then to
  `Emitter::from_legacy_world_units`'s `outer_half_angle_radians`
  (`crates/core/src/lighting.rs:271`). No double conversion. The ESM sibling
  (`light_anim.rs:209`, `(fov * 0.5).to_radians()`) lands in the same units.
  Closed as `#4859`; not a finding.
  - `NiSpotLight.inner_spot_angle` and `.exponent` (the angular distribution
    exponent) are still parsed without a sink. Canonical `Emitter` has no
    inner-cone or angular-exponent lane: its `falloff_exponent` is the
    *radial* shape (`lighting.rs` "Compatibility curve shape";
    `lighting.glsl:104`), so feeding the Gamebryo angular exponent into it
    would be a semantic error, not a fix. `NiSpotLight` is rare in shipped
    Bethesda NIFs, where spots are ESM `LIGH`-driven. Recorded here, not
    filed.
- **`9281eebd7` (#4561) — `APP_CULLED` / editor-marker gate on particle
  leaves (scene-graph flag fidelity).** Verified. `particle_system_is_culled`
  (`import/walk/mod.rs:236`) is called from both walkers
  (`walk_node_hierarchical` and `walk_node_particle_emitters_flat`). Its
  target-keyed fallback downcasts `NiVisController` to
  `NiPreSplitDataController`, and that is the type the dispatcher produces
  for it (`blocks/mod.rs:788-800`), so the fallback is live.
- **`3ce2e1d7b` (#4769)**, `anim_convert.rs` traversal guard: clean.
- **`2b1b7fc5c` / `ab255cfd2`**: dropping `shadow_flags` from visibility
  selection (`LightSource::from_legacy_world_units` → `VisibilityMask::for_legacy_local_light()`)
  and deleting the FO3/FNV zero-projection → omnidirectional synthesis in
  `canonical_light_shadow_flags` are renderer-policy changes (#4557 lineage)
  and belong to `/audit-renderer`. Not re-derived.
- **Animation state machine**: `#4128` is still open and unchanged. Zero
  commits on `crates/core/src/animation/`.

### LC-D2-01: NIF-light spawn path ignores the co-located ESM `LIGH` falloff sentinel, so one `LIGH` record yields k=1.0 or k=2.0 depending on the lamp NIF
- **Severity**: MEDIUM
- **Dimension**: 2 — Legacy subsystem coverage (light translation boundary)
- **Location**: `byroredux/src/cell_loader/spawn.rs:1069-1147` (the literal
  `0.0` falloff at `:1142`); guard premise at
  `byroredux/src/cell_loader/spawn/mesh_instance.rs:1772-1776`
- **Status**: NEW (sibling of closed `#4514`; not caught by its guard)
- **Description**: `canonical_light_falloff_exponent` resolves the LIGH
  `falloff_exponent` "field absent" sentinel (`0.0`) per layout generation:
  **2.0** for Oblivion/FO3/FNV (32-byte LIGH, "FO3/FNV's common quadratic
  k ≈ 2") and 1.0 for Skyrim+ (`systems/light_anim.rs:219-236`). `#4514`
  routed the three ESM-light spawn sites through it. `spawn_nif_lights`
  receives the same REFR's `light_data: Option<&esm::cell::LightData>`
  (`spawn.rs:704`) and uses it as the authoritative radius
  (`esm_radius`, `:1084`, `:1120-1124`). Its falloff, though, is a hard
  `0.0`, which `Emitter`'s last-resort net turns into **1.0**
  (`crates/core/src/lighting.rs:279-283`). The ESM fallback in
  `mesh_instance.rs` only fires when `spawned_nif_lights == 0`
  (`mesh_instance.rs:1527`), so the two routes are mutually exclusive by NIF
  content:
  - A FO3/FNV `LIGH` REFR whose lamp NIF has **no** spawnable `NiLight` →
    ESM fallback → k = **2.0**
  - The same `LIGH` REFR whose NIF has a spawnable `NiLight` →
    `spawn_nif_lights` → radius from the same `LIGH`, k = **1.0**

  The `#4514` source-level guard documents this site as deliberately out of
  scope because it is "a non-ESM producer with no sentinel to resolve". That
  premise holds only on the loose-NIF path (`scene/nif_loader.rs:565`,
  `light_data = None`). On the cell path it is ESM-backed: it already reads
  that record's radius.
- **Evidence**:
  ```rust
  // byroredux/src/cell_loader/spawn.rs:1084, 1120-1124, 1136-1147
  let esm_radius = light_data.as_ref().map(|ld| ld.radius);
  ...
  let raw_radius = match esm_radius {
      Some(r) if r > 0.0 => r * ref_scale,   // ESM LIGH is authoritative here
      ...
  LightSource::from_legacy_world_units(
      radius, light.color,
      byroredux_core::ecs::LIGHT_FLAG_SHADOW_OMNIDIRECTIONAL,
      0.0,                                   // falloff: never canonicalized
      light.kind, world_direction, light.outer_angle,
      byroredux_core::ecs::LIGHT_FLAG_SHADOW_OMNIDIRECTIONAL,
  ),
  ```
  ```rust
  // mesh_instance.rs:1772-1776 (#4514 guard doc)
  /// The fourth `from_legacy_world_units` site (NIF-authored lights in
  /// `spawn_nif_lights`) is a non-ESM producer with no sentinel to
  /// resolve; `Emitter`'s own `1.0` net is its documented contract, ...
  ```
- **Impact**: This is the same visual class `#4514` fixed (MEDIUM there): a
  too-flat falloff curve on classic-game lamps. It hits exactly the lamps
  whose NIF carries a live `NiPointLight`. On FO3/FNV that is the common
  case for meshed lights: `attenuation_radius` documents that 82/82 measured
  FNV NIF lights ship a zero-only attenuation triple, and radius control is
  deferred to the `LIGH` record. Two copies of the same `LIGH` base in one
  cell can render with different falloff shapes if their models differ.
  Skyrim+ is unaffected (its canonical default is also 1.0). Oblivion is
  affected on the same terms as FO3/FNV.
- **Related**: `#4514` (closed; same lane on the ESM-fallback site), `6b4e6252c`
  (the first two sites), `render/lights.rs:57-63`
  (`FALLOFF_EXPONENT_DEFAULT` doc, which also lists "NIF-direct lights" under
  the 1.0 default). Not in `AUDIT_RENDERER_2026-09-23.md` /
  `AUDIT_RENDERER_2026-09-26.md`.
- **Suggested Fix**: Pass a resolved falloff into `spawn_nif_lights`. The
  caller already has `game` and `light_data`. Use
  `canonical_light_falloff_exponent(game, ld.falloff_exponent)` when
  `light_data` is `Some`, and keep the `1.0` net for `None`, which is the
  loose-NIF path and genuinely non-ESM. Extend
  `every_ligh_spawn_site_consumes_the_canonical_falloff_lane` to count this
  site, and correct its "non-ESM producer" prose.

---

## Dimension 3 — Cross-game translation-pattern spot-check

~25 commits in range on `crates/plugin/src/esm/records/` and `reader.rs`,
including the fixes for all three Pattern-C findings cited from
`AUDIT_ESM_2026-09-21.md` last run: #4638 (Oblivion 8-byte `LVLO` /
`LVLD 0x80`), #4639 (per-game TES4 light/medium-master bits) and #4643
(FO76 HEDR).

**Re-ran the first-step checks fresh against HEAD:**
- **Pattern A**: zero production raw-literal `bsver()` thresholds. The three
  grep hits are test `expect`/`assert` strings in `blocks/base.rs:530-551`.
  This matches the skill's pinned baseline.
- **Scattered `game ==`**: one new `GameKind` arm in `cell_loader`,
  `btr_normal_path_candidates` (`cell_loader/terrain_lod_btr.rs:142-157`,
  FO4 `_msn` → `_n` fallback). It's a table-shaped `match` returning data
  (allowed shape), not a scattered branch.
- **Pattern C, new per-game/era decode splits since baseline**. Each was
  checked for a test per era:
  - `#4639` TES4 master flags (`reader.rs`): a per-game `match` table, pinned
    by `tes4_master_flags_decode_per_game` (Starfield 0x100/0x200/0x400, FO4,
    Skyrim SE vs LE by HEDR, Oblivion/FO3/FNV/FO76 no-ESL). Pattern B, done
    correctly.
  - `#4416` IMGS (`records/misc/world.rs`, `decode_image_space`): FO3/FNV
    `DNAM` split by size (132/148 vs 152 with Skin Dimmer), Skyrim/FO4/FO76
    `CNAM`+`TNAM` with the legacy 56-byte `ENAM` fallback, and
    Oblivion/Starfield → `None`. Tests cover every size band and both
    Skyrim-family shapes (`fo3_imgs_dnam_decodes_the_grade_by_size`,
    `skyrim_imgs_reads_cnam_tnam_and_the_legacy_enam`), plus real-ESM
    coverage in `crates/plugin/tests/parse_real_esm.rs`. The FO3/FNV
    cinematic float indices (sat 24, contrast 26, brightness 27, tint 28-31)
    match xEdit's `Saturation, Contrast Avg Lum, Contrast, Brightness, Tint
    RGB, Tint Amount` ordering.
  - `#4645` TRDA (FO4/FO76 vs SF1) and `#4638` LVLO: each lands with
    per-era tests (3 and 4 new `#[test]`s respectively). Depth is
    `/audit-esm`'s charter.

**No new findings.**

---

## Sub-areas with zero commits since baseline (guards spot-checked)

- Dim 1: `crates/nif/src/blocks/strip.rs`, `byroredux/src/cell_loader/euler.rs`,
  `crates/core/src/ecs/components/camera.rs`, `crates/nif/src/rotation.rs`,
  `crates/nif/src/import/coord.rs`.
- Dim 2: `crates/nif/src/import/material/legacy_properties.rs`,
  `crates/nif/src/import/material/walker.rs`, `crates/nif/src/blocks/properties.rs`
  (property → pipeline dispatch unchanged), `crates/core/src/animation/`,
  `crates/core/src/string/`, `byroredux/src/name_lookup.rs`,
  `crates/core/src/ecs/components/world_bound.rs`.

## Files Reviewed

`crates/core/src/math/coord.rs`, `crates/bsa/src/uvd.rs`, `crates/bsa/src/lib.rs`,
`byroredux/src/boot/mod.rs`, `byroredux/src/cell_loader/{euler,load,spawn,terrain,terrain_lod,terrain_lod_btr}.rs`,
`byroredux/src/cell_loader/spawn/mesh_instance.rs`, `byroredux/src/scene/nif_loader.rs`,
`byroredux/src/systems/light_anim.rs`, `byroredux/src/render/lights.rs`,
`crates/core/src/lighting.rs`, `crates/core/src/ecs/components/{light,camera}.rs`,
`crates/nif/src/blocks/{light,strip,mod}.rs`, `crates/nif/src/import/walk/{mod,lights,emitter}.rs`,
`crates/nif/src/import/collision/mod.rs`, `crates/physics/src/ragdoll.rs`,
`crates/plugin/src/esm/reader.rs`, `crates/plugin/src/esm/records/misc/world.rs`,
`docs/engine/coordinate-system.md`, `/mnt/data/src/reference/gamebyro-v32/Include/NiSpotLight.{h,inl}`,
`/mnt/data/src/reference/nifxml/nif.xml` (NiSpotLight), plus issues #4126, #4127, #4514, #4764.

## Suggested Next Step

```
/audit-publish docs/audits/AUDIT_LEGACY_COMPAT_2026-09-27.md
```
Labels: LC-D2-01 → `medium` `bug` `legacy-compat` `renderer` `game:fnv`/`game:fo3`/`game:oblivion`;
COORD-04 → `low` `tech-debt` `legacy-compat` `import-pipeline`. Also consider closing
`#4127` (doc now matches code).
