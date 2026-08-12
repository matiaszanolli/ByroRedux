# NIFAL Audit — 2026-08-12

**Scope: Dimensions 1 and 8 ONLY** (`/audit-nifal --focus 1,8`, run as part of
the `texture-roles-deep` suite preset). Dimensions 2-7 and 9
(Geometry/Transform, Skinning/Lights, Nodes, Particles, Collision, Animation,
Completeness) were **not** run and nothing here should be read as coverage of
them.

- **Dimension 1** — Material: the single `translate_material` boundary, PBR
  resolve-once, glass-once.
- **Dimension 8** — Shader flags / texture sets / effect shaders, with the
  2026-07-27 `MaterialTextureSet<T>` role unification as the primary subject.

Games with data present and exercised: **Skyrim SE** (measured, `Skyrim -
Meshes0.bsa` + `Meshes1.bsa`). FO4 / FO76 / Starfield / Oblivion / FO3 / FNV
reasoned about from shared code paths only.

Dedup baseline: `/tmp/audit/issues.json` (258 issues, pre-fetched) plus the 13
prior `AUDIT_NIFAL_*` reports in `docs/audits/`.

---

## Executive Summary

The material boundary itself is in good shape: **Dimension 1 produced no
behavioural findings**. `translate_material` still has the narrowed
`&ImportedMaterial + mesh_name` signature, exactly two production callers, plain
resolved `f32` PBR with the correct clamps, glass classified once after
`resolve_pbr`, and no render-time classifier anywhere. The one Dimension-1
finding is documentation drift on the canonical `Material` type.

Dimension 8's *structural* contract also holds — `values()` matches the struct
field-for-field (the audit's headline regression surface is clean), `map_ref` is
compiler-protected, `smooth_spec`/`specular` and `environment`/`environment_mask`
are distinct at every stage, the cell-unload lifecycle sweep is exhaustive via
`secondary_values()`, and `triangle.frag` has zero per-game branches.

What is **not** clean is the *content* of the slot→role decision. Measuring
vanilla Skyrim rather than re-reading the code found that the importer's
`BSShaderTextureSet` slot table is derived from nif.xml's enum prose, which
contradicts nif.xml's own field table and the shipped data. Slot 6 is never read
anywhere in the importer, yet it carries the MultiLayerParallax inner layer (662
authored strings) and the FaceGen face tint (3 150 authored strings). The
FaceTint arm reads two slots that are empty on 100 % of vanilla content while the
three slots that are populated all land in the wrong canonical role — including
a face *detail* map bound as a parallax height field that `triangle.frag`
actually ray-marches.

Violation counts (Dim 1 + Dim 8 only):

| Tier invariant | Violations found |
|---|---|
| `single-boundary` | 1 (two disagreeing `BSShaderTextureSet` slot→role tables) |
| `no-fabrication` | 0 |
| `no-leak` (authored data dropped / bound to the wrong canonical role) | 3 |
| `no-render-time-fallback` | 0 |
| documentation / regression-surface only | 2 |

---

## Per-Category Tier Matrix

| Category | Boundary fn | single-boundary | no-fabrication | no-leak | no-render-time-fallback |
|---|---|---|---|---|---|
| Material | `byroredux/src/material_translate.rs::translate_material` | PASS (2 callers, 1 site) | PASS (emissive still a no-op) | PASS | PASS |
| Texture roles — `MaterialTextureSet<T>` mechanics | `crates/nif/src/import/types.rs` (`map_ref` / `values` / `secondary_values`) | PASS | PASS | PASS | N/A |
| Texture roles — NIF slot→role decision | `crates/nif/src/import/material/dedicated_shader.rs` | **FAIL** (duplicated in `byroredux/src/cell_loader/refr.rs`) | PASS | **FAIL** (slots 2/3/6/7) | PASS |
| Texture roles — REFR TXST/XTXR overlay | `byroredux/src/cell_loader/refr.rs` + `byroredux/src/cell_loader/spawn.rs::resolve_mesh_paths` | **FAIL** (second table) | PASS | **FAIL** (`inner` dropped) | PASS |
| Texture roles — external material merge | `byroredux/src/asset_provider/material.rs::merge_external_material` | PASS | PASS | pre-existing (#2627, #2594, #2608) | PASS |
| Shader flags / effect shaders | `crates/nif/src/shader_flags.rs` + `dedicated_shader.rs` | PASS | PASS | PASS | PASS |
| Renderer role transport | `byroredux/src/render/static_meshes.rs` → `crates/renderer/src/vulkan/material.rs` | PASS (correct today) | PASS | PASS | PASS — but untested ordering (D8-05) |

---

## Findings

### HIGH

#### NIFAL-D8-2026-08-12-01: MultiLayerParallax inner layer is read from `BSShaderTextureSet` slot 7; shipped content authors it in slot 6
- **Severity**: HIGH
- **Dimension**: Shader-flags/Effects (texture roles)
- **Tier Violated**: `no-leak` (authored role dropped) — and a wrong canonical texture-role output, which the severity table floors at HIGH
- **Game Affected**: Skyrim SE/LE (measured); FO4/FO76 share the `shader_type == 11` arm (unmeasured)
- **Location**: `crates/nif/src/import/material/dedicated_shader.rs:168-189`
- **Status**: NEW
- **Description**: The `11 =>` arm routes texture-set slot 7 into
  `MaterialInfo::inner_layer_map` → canonical `MaterialTextureSet::inner_layer` →
  `GpuMaterial.inner_layer_map_index` → `crates/renderer/shaders/triangle.frag`
  (`materialKind == 11u` branch). Vanilla Skyrim authors the inner layer in slot
  **6**, which the importer never reads at all — `textures.get(6)` appears nowhere
  under `crates/nif/src/`.
- **Evidence** — `crates/nif/examples/_tmp_nifal_d8_mlp.rs` over
  `Skyrim - Meshes0.bsa` (607 MLP properties) and `Meshes1.bsa` (55, i.e. 100 % of
  that archive's MLP shapes):
  ```
  slot 6: 607 + 55 non-empty
      textures\architecture\windhelm\WHwindowinner02.dds
      textures\architecture\solitude\Sinside.dds
      textures\dungeons\caves\IceCaveWall02.dds
  slot 7: 370 + 10 non-empty
      textures\dungeons\caves\IceCaveSubsurfacetint01.dds
  ```
  Three independent corroborations that slot 6 is the inner layer: the filenames
  themselves (`WHwindowinner02`, `Sinside` = Solitude interior); nif.xml's
  `BSShaderTextureSet` field table, which documents slot 6 as *"Subsurface for
  Multilayer Parallax"* and slot 7 as *"Back Lighting Map
  (SLSF2_Back_Lighting)"* (niftools nif.xml, lines 6307-6319 at
  /mnt/data/src/reference/nifxml/nif.xml); and this engine's own REFR overlay
  table, which already maps NIF slot 6 → `inner`
  (`byroredux/src/cell_loader/refr.rs:157`). The arm's comment cites nif.xml's
  *enum* prose ("Layer(TS7)", same file line 1413) — the one statement the data
  contradicts.
- **Impact**: Every Skyrim multilayer-parallax surface (ice caves and glaciers,
  Windhelm/Solitude/ship windows) samples its parallax inner layer from the
  subsurface/backlight tint map, and the authored inner layer is never uploaded.
  No downstream fallback masks it.
- **Related**: NIFAL-D8-2026-08-12-04; #2627 (the BGSM half of the same canonical role).
- **Suggested Fix**: Read slot 6 into `inner_layer_map` in the `11 =>` arm and
  decide slot 7's canonical home separately (back-lighting role, or an explicit
  park). Pin with a fixture test asserting slot 6 → `inner_layer` for shader type 11.

#### NIFAL-D8-2026-08-12-02: FaceTint reads two slots vanilla content never authors, and misroutes the three it does
- **Severity**: HIGH
- **Dimension**: Shader-flags/Effects (texture roles)
- **Tier Violated**: `no-leak` (three authored roles dropped or bound to the wrong canonical role)
- **Game Affected**: Skyrim SE/LE (measured); FO4 shares the `shader_type == 4` arm
- **Location**: `crates/nif/src/import/material/dedicated_shader.rs:106-125` (slot-2 gate), `:132-136` (slot 3), `:148-167` (the `4 =>` arm)
- **Status**: NEW
- **Description**: The FaceTint arm reads slot 4 → `detail_map` and slot 7 →
  `tint_map` (nif.xml enum prose again). Both slots are empty on **100 %** of
  vanilla Skyrim FaceTint properties, so the arm is inert, while the three
  populated slots each land wrong:
  - slot 2 (`*_sk.dds` skin-tint mask, 3158/3158) → `glow_map` → canonical
    `emissive`, because the `skin_tint_slot` gate only fires for `shader_type == 5`
    / `ShaderTypeData::SkinTint`, and Skyrim FaceTint parses as
    `ShaderTypeData::None` (`crates/nif/src/blocks/shader.rs:594-597`).
  - slot 3 (`FemaleHeadDetail_Age40.dds`, `BlankDetailmap.dds`, 3149/3158) →
    `parallax_map`, and `crates/renderer/shaders/triangle.frag` runs
    parallax-occlusion displacement whenever that index is non-zero — there is no
    `materialKind` gate on the POM branch.
  - slot 6 (`…\FaceGenData\FaceTint\Skyrim.esm\<formid>.dds`, 3150/3158) → nothing.
- **Evidence** — `crates/nif/examples/_tmp_nifal_d8_mlp.rs … 4` over
  `Skyrim - Meshes0.bsa`: 3158 FaceTint properties across 3158 NIFs, non-empty
  counts `0:3158, 1:3158 (_msn 3113 / _n 45), 2:3158 (_sk 3158), 3:3149, 6:3150`;
  slots 4, 5 and 7 never appear.
- **Impact**: Every vanilla Skyrim head binds its skin-tint mask as the glow map
  (latent while `emissive_color` is black — one authored non-black value away from
  glowing faces), ray-marches POM from a face detail map used as a height field,
  and drops the per-NPC FaceGen tint the NIF points at (the canonical `tint` role
  is live and sampled).
- **Related**: NIFAL-D8-2026-08-12-01 (same root cause); #2095 (the FaceGen
  diffuse override path, which is how tint reaches faces today).
- **Suggested Fix**: In the FaceTint arm route slot 2 → `tint`, slot 3 → `detail`,
  slot 6 → `tint`/FaceGen (deciding precedence against `select_facegen_diffuse`),
  and stop feeding slot 3 into `parallax_map` for this shader type.

### MEDIUM

#### NIFAL-D8-2026-08-12-03: `RefrTextureOverlay.inner` is populated by TXST + XTXR and has zero consumers
- **Severity**: MEDIUM
- **Dimension**: Shader-flags/Effects (texture roles)
- **Tier Violated**: `no-leak` — authored override captured, then dropped at the spawn→translate boundary
- **Game Affected**: Skyrim SE, FO4, FO76 (every TXST-bearing REFR)
- **Location**: written at `byroredux/src/cell_loader/refr.rs:65`, `:120`, `:157`, `:172`; never read — `byroredux/src/cell_loader/spawn.rs:1149-1219`
- **Status**: NEW
- **Description**: `RefrTextureOverlay` carries a resolved `inner` role, filled by
  both the whole-TXST merge and the `XTXR` slot-6 swap. `resolve_mesh_paths`, the
  overlay's only consumer, applies `diffuse`, `normal`, `glow`,
  `specular`/`smooth_spec`, `height`, `env`, `env_mask`, `wrinkle` and
  `material_path` — and never assigns `textures.inner_layer`.
- **Evidence**: `grep -rn "o\.inner" byroredux/` returns nothing, while every
  sibling role appears at `byroredux/src/cell_loader/spawn.rs:1158-1213`.
- **Impact**: ESM-level retextures of the multilayer inner layer silently fall
  back to the base NIF texture. Bounded to one role on the override path, hence
  MEDIUM rather than HIGH.
- **Related**: NIFAL-D8-2026-08-12-01 (base-path half of the same role).
- **Suggested Fix**: Assign `textures.inner_layer` alongside the other eight and
  add a test asserting every `RefrTextureOverlay` field has a consumer.

#### NIFAL-D8-2026-08-12-04: Two independent `BSShaderTextureSet` slot→role tables that already disagree
- **Severity**: MEDIUM
- **Dimension**: Shader-flags/Effects (texture roles)
- **Tier Violated**: `single-boundary`
- **Game Affected**: Skyrim SE, FO4, FO76
- **Location**: `crates/nif/src/import/material/dedicated_shader.rs:97-238` (shader-type-aware) vs `byroredux/src/cell_loader/refr.rs:139-180` (shader-type-agnostic)
- **Status**: NEW
- **Description**: The importer resolves slots 2/4/7 differently per `shader_type`;
  the REFR overlay resolves the same NIF slot indices through one fixed table
  (`0→diffuse, 1→normal, 2→glow, 3→height, 4→env, 5→env_mask, 6→inner,
  7→specular`) and never sees `shader_type`. The two already disagree on slot 6
  (the overlay is the correct one — see D8-01) and on slots 2/4/7 for shader types
  4/5/11.
- **Evidence**: the two `match` blocks side by side; D8-01 measures which is right
  for slot 6.
- **Impact**: An XTXR swap on a FaceTint / SkinTint / MultiLayerParallax placement
  lands in a different canonical role than the same slot read from the mesh's own
  texture set, so an override changes shading semantics rather than just the
  texture — and any fix to one table silently fails to propagate to the other.
- **Related**: D8-01, D8-03.
- **Suggested Fix**: One `slot_to_role(shader_type, slot)` helper in `crates/nif`,
  called by both sites; the overlay gets `shader_type` from the cached import.

### LOW

#### NIFAL-D1-2026-08-12-01: Canonical `Material` doc cites a `grayscale_to_palette_scale` precedent field that does not exist on `Material`
- **Severity**: LOW
- **Dimension**: Material
- **Tier Violated**: none (documentation defect on the canonical type)
- **Game Affected**: all (doc only)
- **Location**: `crates/core/src/ecs/components/material.rs:256-260`
- **Status**: NEW
- **Description**: The #2284 rationale block says the six BSLSP shading scalars
  landed on `Material` "matching the existing `grayscale_to_palette_scale`
  precedent (see that field's doc …)". No such field exists on `Material` — the
  string occurs exactly once in that file, inside this comment. The authored
  scalar lives on the raw `ImportedMaterial` (`crates/nif/src/import/types.rs`,
  written by `byroredux/src/asset_provider/material.rs:1058`) and is
  raw-tier-parked — a *different* tier from the precedent claimed.
- **Evidence**: `grep -c grayscale_to_palette_scale crates/core/src/ecs/components/material.rs` → 1.
- **Impact**: A future audit reading the canonical type's own docs is told a field
  exists that does not, obscuring the genuine parked-at-raw-tier status. No
  runtime effect.
- **Related**: the accurate anchor is the "not yet plumbed to GpuMaterial" comment
  in `crates/renderer/shaders/triangle.frag`.
- **Suggested Fix**: Reword to say the precedent is parked one tier lower on
  `ImportedMaterial`, or land the field for real.

#### NIFAL-D8-2026-08-12-05: `supplemental_texture_indices` is a third hand-written role walk with no lockstep test
- **Severity**: LOW
- **Dimension**: Shader-flags/Effects (texture roles)
- **Tier Violated**: none today (verified correct) — regression surface only
- **Game Affected**: all
- **Location**: `byroredux/src/render/static_meshes.rs:561-574` vs `crates/renderer/src/vulkan/material.rs:415-430` and `crates/renderer/src/vulkan/context/mod.rs:492-504`
- **Status**: NEW
- **Description**: Beyond the two role walks the spec names (`map_ref`,
  compiler-protected; `values()`, not), there is a third: a positional `[u32; 12]`
  built in `byroredux` and indexed back out through `supplemental_texture_slot::*`
  constants in `byroredux_renderer`. Nothing couples the two orders. Verified
  correct today (tint, inner_layer, specular, lighting, flow, wrinkle,
  reflectance, emittance_gradient, decals 0-3), and the GPU side is protected by
  `material_hash_matches_gpu_material_field_hash` plus the `offset_of!` pins — but
  the CPU-side ordering has no test at all.
- **Evidence**: `grep -rn supplemental_texture_slot --include='*.rs' | grep -i test` → no hits.
- **Impact**: Inserting a constant mid-list silently shifts every following role by
  one — tint sampled as specular, etc. — with no compile error and no failing test.
- **Related**: the `values()` regression surface documented in `docs/engine/nifal.md`.
- **Suggested Fix**: Index the constants when building the array
  (`arr[slot::TINT] = …`), or add an explicit ordering test.

---

## Documented-limitation ledger (verified parked, NOT findings)

Restated so the next sweep does not re-report them:

- **Emissive scale is a measured no-op** — all three `EmissiveSource` variants
  cluster at ~1.0 (`docs/engine/nifal.md` §4). A normalization constant would be a
  `no-fabrication` violation. Verified: no such constant exists in the boundary.
- **`material_kind: u32` stays a `u32`** — it is the `triangle.frag` dispatch
  contract, deliberately not an enum.
- **SLSF1 `Refraction` without `Fire_Refraction`** has no engine consumer;
  `refraction_strength` deliberately does not ride `Material.ior`
  (`byroredux/src/material_translate.rs:29-69`, #2327).
- **Six BSLSP shading scalars** (`lighting_effect_1/2`, `subsurface_rolloff`,
  `rimlight_power`, `backlight_power`, `fresnel_power`) are captured on the
  canonical `Material` with no GPU consumer yet (#2284) — deliberate.
- **`ImportedMaterial.grayscale_to_palette_scale`** is raw-tier-parked with the
  deferral documented shader-side; only the doc reference to it is wrong (D1-01).
- **Starfield `.mat` / CDB** contributes no texture roles — the merge returns
  early after flipping `is_pbr` (`byroredux/src/asset_provider/material.rs:726-739`),
  tracked by #2359. Not re-reported.
- **`lighting` / `flow` / `wrinkle` roles** reach the GPU contract but are
  deliberately unsampled until their authored lookup coordinates exist
  (`docs/engine/nifal.md`, texture-roles section).
- **Pre-existing OPEN issues touching these two dimensions**, all re-verified as
  still-accurate and skipped: #2330 / #2572 (roughness written at a second
  spawn-time site), #2444 (exterior draw populations never reach
  `translate_material`), #2573, #2606, #2607, #2608, #2627, #2642, #2533, #2594,
  #2359, #2641 / #2591 / #2556 (`EmissiveSource::None` doc contradictions).

---

## Measurement tooling added

`crates/nif/examples/_tmp_nifal_d8_mlp.rs` — per-`shader_type`
`BSShaderTextureSet` slot census over a BSA (non-empty counts, filename-suffix
histogram, samples). This is the harness that produced the D8-01 / D8-02
evidence; it follows the existing `_tmp_*` throwaway-probe convention in that
directory.

```
cargo run --release -p byroredux-nif --example _tmp_nifal_d8_mlp -- "<path>/Skyrim - Meshes0.bsa" 11
```
