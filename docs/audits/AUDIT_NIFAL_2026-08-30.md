# NIFAL Audit — 2026-08-30

Scope: all 9 dimensions of the canonical-translation tier, all games. Executed
in-process (no fan-out) by reading, grepping and mechanically diffing the live
tree. Baseline for the delta: `docs/audits/AUDIT_NIFAL_2026-08-27.md`, whose five
findings were all closed by `d5a8c36c`; the code delta reviewed is
`d5a8c36c..HEAD` (40 commits).

No cargo invocation was made during this audit — the host was under a hard memory
constraint for the duration. Every tier invariant this report checks is decidable
by static analysis, and each finding below cites file and line. The one thing this
forecloses is running the `#[ignore]`d per-game fill-rate harness, which is a lead
generator rather than a gate.

## Executive Summary

**4 findings: 0 CRITICAL, 0 HIGH, 2 MEDIUM, 2 LOW.**

The headline is a clean one: **the cardinal NIFAL invariant holds end to end.**
Two independent scans — every `*.frag`/`*.vert`/`*.comp` plus every
`include/*.glsl`, and `grep -rniE "GameKind::|game ==" crates/renderer/src
byroredux/src/render/` — return **zero** per-game runtime branches in either
language. Every game-name hit in the shader tree is an explanatory comment. Per-game
divergence is genuinely being translated away at the parser→Material boundary rather
than leaking downstream.

The busiest surface in this window was the newest code: #3530 wired Oblivion's
`APPLY_HILIGHT2` parallax route through `Material::parallax_height_in_alpha`. I
audited it hard, because it is exactly the shape that usually goes wrong, and it is
**correct**: the per-game decision is made once at the NIFAL boundary from
`NiTexturingProperty.apply_mode`, the renderer only transports it as bit 31, both the
raster and RT shader paths mask it off symmetrically, and it deliberately reuses the
existing `0.04 / 4.0` engine defaults rather than inventing an Oblivion-specific
constant. Collision is 16/16 shape arms with a genuine structural guard; particles,
geometry/transform, nodes and lights are all clean.

The two MEDIUMs are both defence-in-depth gaps in the *newest* additions to
otherwise-converged categories, and both have the same signature: a mechanism that
was extended correctly in one place and not in its sibling.

- **NIFAL-2026-08-30-D1-01** — `Material::sanitize_finite` covers all 33 of the
  canonical material's directly-declared float fields (I diffed them mechanically;
  #3373's specific hole is closed) but never descends into the two float payloads
  behind indirection: `effect_falloff`'s 5 scalars and `shader_type_fields`' 13. That
  is 22 slots outside both save-path gates, all of which reach `GpuMaterial`, on a
  path where the parser applies no finiteness guard of its own.
- **NIFAL-2026-08-30-D8-01** — `#3458`'s slot-2 colocation fix (the 08-27 HIGH) was
  wired into the NIF import loop but not into the REFR-overlay sibling, whose `pick`
  closure consults only `slot_to_role` and therefore *structurally cannot* express a
  colocated role. Vanilla reachability today is very low, which is why it is not a
  re-run of the HIGH — but any future colocation will fail the same silent way.

Both LOWs are latent rather than live: an exterior spawner outside the #2444 boundary
guard, and the one role walk whose test is not drift-proof.

**Stale candidates dropped: 8** across the nine dimensions (detail in each dimension's
section below, and summarised at the end). Two are worth flagging up front because they
were carried *forward* in the previous report's ledger and are now fixed: `#2610`
(particle `effect_shader_flags` hardcoded `0`) and FNV-D2-03 (`terrain_lod_btr.rs`
spawning without a canonical `Material`). One more, `BhkSimpleShape` as a 17th
unresolved collision shape, was my own greedy-grep artefact and is recorded as such.

## Per-Category Tier Matrix

| Category | single-boundary | no-fabrication | no-leak | no-render-time-fallback | Boundary / notes |
|---|---|---|---|---|---|
| Material (Dim 1) | PASS | PASS | **FAIL** — D1-01 | PASS | `translate_material`, 3 production callers; signature still narrowed to `&ImportedMaterial` |
| Material, no-source-record (Dim 1) | **partial** — D1-02 | PASS | PASS | PASS | `translate_texture_only_material`; guard enumerates 5 of 6 cell-loader spawners |
| Material markers (Dim 1) | PASS | PASS | PASS | PASS | `attach_blend_and_facing_markers`, both spawn sites, source-scan pinned |
| Mesh water (Dim 1) | PASS | PASS | PASS | — | `attach_mesh_water`, both consumers |
| Geometry / Transform (Dim 2) | PASS | PASS | PASS | PASS | the reference template; zero coord handling in the renderer |
| Skinning (Dim 3) | PASS | — | documented gap (#2440, unchanged) | — | one production `SkinnedMesh::new_with_global` |
| Lights (Dim 3) | PASS | PASS | PASS | — | `translate_light`, all 3 ESM producers |
| Nodes (Dim 4) | N/A by design | — | PASS — 7 parked fields, **0** canonical consumers | — | re-grepped field by field |
| Particles (Dim 5) | PASS | PASS | PASS | PASS | `apply_emitter_overlays`; absorbed #2300 + #3344 this window |
| Collision (Dim 6) | PASS | PASS | PASS | — | **16/16** arms, byte-identical set diff; `dispatch_coverage_tests` is a real guard |
| Animation (Dim 7) | PASS | PASS | partial — `duration`/`weight` (owned by **#3432**) | — | `convert_nif_clip` + declared `convert_hkx_clip`; `phase` newly sanitised (#3345) |
| Shader flags / texture roles (Dim 8) | **FAIL** — D8-01 | PASS | PASS (latent gap: D8-02) | PASS | 22 named + 4 decal roles; `slot_to_role` table fully corpus-sourced |
| Cross-cutting (Dim 9) | PASS | PASS | — | **PASS** — zero per-game branches in GLSL *and* Rust | harness limits already filed (#2532, #3462) |

## Findings

### MEDIUM

#### NIFAL-2026-08-30-D1-01: `Material::sanitize_finite` sweeps only the 33 top-level scalars and never descends into `effect_falloff` / `shader_type_fields` — 22 further float slots that reach `GpuMaterial` unrepaired
- **Severity**: MEDIUM
- **Dimension**: Material
- **Tier Violated**: no-leak
- **Game Affected**: all (FO3/FNV via `BSShaderNoLightingProperty` falloff; Skyrim+/FO4 via `BSEffectShaderProperty` falloff and the `BSLightingShaderProperty` shader-type payloads)
- **Location**: `crates/core/src/ecs/components/material.rs:1215-1278` (the sweep), `:232` / `:224` (the two uncovered carriers), `:445-455` (`EffectFalloff`), `:467-481` (`ShaderTypeFields`)
- **Status**: NEW (same defect class as the closed #3373; related to #3438, which owns the *pin*, not the sweep)
- **Description**: `sanitize_finite` is the single finiteness gate for the canonical `Material`, consumed by `crates/save/src/driver.rs:145-154` on restore and probed on a clone by `validate_material_finiteness` (`crates/save/src/validate.rs:450-456`) pre-save. Its macro list covers every *directly-declared* float field — I diffed it mechanically and all 33 (31 explicit + `metalness`/`roughness` via `resolve_pbr`) are present, so #3373's specific hole is closed. But `Material` carries two further float payloads behind indirection that the macro list cannot reach and does not mention:
  - `effect_falloff: Option<EffectFalloff>` — 5 f32 (`start_angle`, `stop_angle`, `start_opacity`, `stop_opacity`, `soft_falloff_depth`);
  - `shader_type_fields: Option<Box<ShaderTypeFields>>` — 13 `Option<f32>`/`Option<[f32; N]>` (`skin_tint_color`, `skin_tint_alpha`, `hair_tint_color`, `eye_cubemap_scale`, `eye_left/right_reflection_center`, `parallax_max_passes`, `parallax_height_scale`, `multi_layer_*` ×4, `sparkle_parameters`).
  That is 22 additional scalar slots outside both save-path gates.
- **Evidence**: The values are live on the GPU path, not inert:
  - `byroredux/src/render/static_meshes.rs:631-633` reads `m.effect_falloff` into `DrawCommand.effect_falloff` (gated on `material_kind == MATERIAL_KIND_EFFECT_SHADER` — i.e. exactly the materials that author a falloff cone), which `crates/renderer/src/vulkan/context/mod.rs:485-489` unpacks into `GpuMaterial.falloff_start_angle` … `soft_falloff_depth`, and `:665` hashes with `to_bits()` into the material-table dedup key.
  - `byroredux/src/render/static_meshes.rs:549-598` reads `shader_type_fields` into the `skin_tint_rgba` / `hair_tint_rgb` / `sparkle_rgba` / `multi_layer_*` GPU slots.
  Reachability: the parser applies no finiteness guard on this path — `NifStream::read_f32_le` (`crates/nif/src/stream.rs:173-177`) returns `f32::from_le_bytes` verbatim for any bit pattern, and the only `is_finite` check in the shader-block parser is the unrelated FO4 rimlight sentinel at `crates/nif/src/blocks/shader.rs:1081-1082`.
- **Impact**: A non-finite authored/corrupted value in an effect-shader falloff cone or a `BSLightingShaderProperty` shader-type payload reaches `GpuMaterial` and the fragment shader unrepaired, and survives a save/load round trip that the same method exists to make safe for its 33 siblings. NaN/Inf into the GPU is exactly the hazard #2687 introduced this method for. Silent: no compile error, no test failure, and the pre-save probe reports the material clean.
- **Related**: #3373 (the identical omission for the BGEM glass-optics tail, fixed), #3438 (the pin cannot catch this class structurally), #3073 (`parallax_height_scale`/`parallax_max_passes` bypass the canonical `Material` — the *same two fields*, different defect).
- **Suggested Fix**: Give `EffectFalloff` and `ShaderTypeFields` their own `sanitize_finite` returning `changed`, and call both from `Material::sanitize_finite` (`if let Some(f) = self.effect_falloff.as_mut() { changed |= f.sanitize_finite(); }`). No new constants — reset to each type's `Default`, matching the existing `fix_scalar!` semantics.

#### NIFAL-2026-08-30-D8-01: `#3458`'s slot-2 colocation was wired into the NIF import loop but not into the REFR-overlay sibling, whose `pick` closure structurally cannot express a colocated role
- **Severity**: MEDIUM (defence-in-depth gap; see the reachability note under Impact — the *live* mis-render on vanilla content is near-nil, the structural inability to propagate is what earns the rating)
- **Dimension**: Shader-flags/Effects
- **Tier Violated**: single-boundary
- **Game Affected**: Skyrim, Starfield (the `TextureSlotLayout` arms `slot_to_colocated_role` covers)
- **Location**: `byroredux/src/cell_loader/spawn/mesh_instance.rs:197-199` (the `pick` closure), `:254-257` (the `lighting_mask` line it gates); contrast `crates/nif/src/import/material/dedicated_shader.rs:222,252`
- **Status**: NEW
- **Description**: `#3458` (fixed 2026-08-28, `d5a8c36c`) established that Skyrim's slot 2 is genuinely **two roles at once** on the tint family — the `*_sk.dds` is both the `Tint` map and the `LightingMask` the `SLSF2_Soft_Lighting` gate asserts exists — and introduced `slot_to_colocated_role` to return the second role. The NIF import loop consults both functions (`dedicated_shader.rs:222` then `:252`, first-wins).
  The REFR texture-overlay path did not get the same treatment. `resolve_mesh_paths` routes every override through one closure:
  ```rust
  let pick = |slot: u32, raw: Option<FixedString>, role: TextureRole| {
      raw.filter(|_| slot_to_role(slot_context, slot) == Some(role))
  };
  ```
  `pick` consults **only** `slot_to_role`. On the tint family `slot_to_role(ctx, 2)` returns `Tint`, so the `lighting_mask` line at `:254-255` — `pick(2, o.glow, TextureRole::LightingMask)` — can never match, and the override is silently dropped for exactly the population `#3458` was about. For non-tint meshes `slot_to_role` does return `LightingMask`, so that arm works; the hole is tint-family-only.
- **Evidence**: `slot_to_colocated_role` (`crates/nif/src/import/material/slot_role.rs:264-277`) is referenced at exactly one non-test site, `dedicated_shader.rs:252`. `grep -rn "slot_to_colocated_role" byroredux/src` returns nothing.
- **Impact**: A REFR whose TXST overrides slot 2 on a Skyrim/Starfield tint-family mesh with `soft_lighting`/`rim_lighting` set updates `tint` but leaves `lighting_mask` bound to the **base mesh's** original slot-2 texture, while `MAT_FLAG_SOFT_LIGHTING` crosses regardless — a half-overridden pair. **Vanilla reachability is very low and I want to be explicit about that**: FaceGen heads reach the engine through `npc_spawn`, not through REFR placement, so the reachable set is REFR-placed statics that use shader type 4/5/6 *and* carry a TXST override *and* set a soft/rim gate. I found no reason to believe that population is non-empty in vanilla, which is why this is LOW rather than a re-run of #3458's HIGH.
  The durable defect is structural, not statistical: `pick`'s signature cannot express a colocated role at all, so **any** future entry added to `slot_to_colocated_role` will silently fail to reach the overlay path the same way, with no compile error and no test.
- **Related**: #3458 (the import-side half, fixed), #3187 (`apply_slot_swap`, the *third* slot table on this same overlay path — still open).
- **Suggested Fix**: Change `pick` to `slot_to_role(slot_context, slot) == Some(role) || slot_to_colocated_role(slot_context, slot) == Some(role)`. One line, and it makes the overlay path track the slot table's colocation model automatically.

### LOW

#### NIFAL-2026-08-30-D1-02: ESM-sourced water planes are the one cell-loader mesh spawner outside the #2444 boundary guard, so their draws shade against the render path's hardcoded-literal arm
- **Severity**: LOW
- **Dimension**: Material
- **Tier Violated**: single-boundary
- **Game Affected**: all (every exterior/interior CELL with an XCWT/LOD water plane)
- **Location**: `byroredux/src/cell_loader/water.rs:481` and `:813` (the two spawn sites), `byroredux/src/material_translate.rs:1609-1654` (the guard that does not list them)
- **Status**: NEW
- **Description**: `#2444` established the invariant "every drawn surface's canonical material is produced at one boundary" and pinned it with the source-scan guard `every_exterior_spawner_inserts_a_boundary_material`, whose own comment says to *"keep this table in step with `cell_loader`'s spawners."* The table lists five. `cell_loader/water.rs` is a sixth cell-loader spawner that inserts a `MeshHandle` (`:481` for the CELL water plane, `:813` for the LOD water plane) and attaches `WaterPlane` + `WaterMaterial` but **no canonical `Material`** — and it carries no documented exemption.
  This matters because water is not drawn by a separate collection pass: `byroredux/src/render/water.rs:111-138` looks up an **already-emitted** `DrawCommand` for each `WaterPlane` entity and merely flips `is_water = true`. So these entities do pass through `collect_static_mesh_draws`, hit the `else` arm at `byroredux/src/render/static_meshes.rs:367-375`, and are interned with `roughness 0.5`, `metalness 0.0` and the default IOR — the same 11-tuple of literals #2444 was filed to eliminate.
  Contrast the NIF-sourced water path, which does not have this gap: `material_translate.rs::attach_mesh_water` attaches a canonical `Material` from `translate_material` alongside `WaterPlane`. The two water producers therefore disagree about whether a water surface has a canonical material — the "two paths silently diverge" shape the boundary exists to prevent.
- **Evidence**: `grep -n "MeshHandle" byroredux/src/cell_loader/water.rs` → `:481`, `:813`; the same file's only material construction is `material: WaterMaterial::default()` at `:958`. The guard's array at `material_translate.rs:1619-1645` enumerates `terrain.rs`, `terrain_lod.rs`, `object_lod.rs`, `placement_lod.rs`, `terrain_lod_btr.rs` and stops.
- **Impact**: Bounded today — `water.frag` derives its optics from the WATR-driven `WaterMaterial` and never reads `mat.roughness`/`mat.metalness` (verified: no such read in `crates/renderer/shaders/water.frag`), so the primary water pass is unaffected. The exposure is the secondary/RT path, which reads the interned `GpuMaterial` generically (`triangle.frag:3756` shades a hit against `hitMat.roughness`/`hitMat.metalness`/`hitMat.ior`), and the durable defect is that the guard no longer enumerates the spawner set it claims to.
- **Related**: #2444 (MAT-D3-02), #3336 (added `terrain_lod_btr` to the same table for the same reason), `docs/engine/watal.md`.
- **Suggested Fix**: Either route both water spawn sites through `translate_texture_only_material` (they have a bound normal/noise texture path) and add `cell_loader/water.rs` to the guard's table, or add it to the table with an explicit third boundary_fn marker recording the deliberate `WaterMaterial`-only exemption. The guard must enumerate the spawner, either way.

#### NIFAL-2026-08-30-D8-02: `values()` — the walk NIFAL designates the exhaustive lifecycle contract — is the one role walk whose test is not drift-proof, three days after its newer sibling `roles()` got exactly the right guard
- **Severity**: LOW
- **Dimension**: Shader-flags/Effects
- **Tier Violated**: no-leak (latent)
- **Game Affected**: all
- **Location**: `crates/nif/src/import/types.rs:425-452` (`values()`), `:467-501` (`canonical_iteration_covers_every_role_once`); contrast `:1721-1734` (`roles_covers_every_field_in_the_set`)
- **Status**: NEW
- **Description**: `MaterialTextureSet` now carries **three** hand-written role walks plus the generic one:
  | Walk | Protected? | By what |
  |---|---|---|
  | `map_ref` (`:392-418`) | **yes** — compiler | builds a full struct literal; a forgotten role is a compile error |
  | `roles()` (`:358-388`, added #3349 this window) | **yes** | `roles_covers_every_field_in_the_set` cross-checks `roles().count()` against `map_ref`'s visit count |
  | `values()` (`:425-452`) | **no** | see below |
  `values()`'s test builds a literal with sequential integers and asserts `values() == (0..26)`. That catches a *reordering*, but not an *omission*: add a 23rd role to the struct, give it `26` in the test literal (which the compiler forces you to do), and forget it in `values()` — `values()` still yields the 26 elements `0..=25`, the assert still passes, and the new role is silently absent from every lifecycle consumer.
  The fix pattern already exists in the same file, ten lines of it, written three days ago for `roles()`.
- **Evidence**: I verified the current lists are correct — 22 named roles in the struct, the same 22 in `values()` in the same order, `+ 4` decals = 26; `secondary_values()`'s `skip(1)` correctly assumes `base_color` is element 0. This is a latent test gap, not a live drop.
- **Impact**: If it ever fires: `docs/engine/nifal.md` designates `values()`/`secondary_values()` the exhaustive lifecycle contract and cell unload uses it directly for texture release, so a role missing from `values()` leaks its texture handle on every cell unload — a compounding GPU resource leak with no compile error and no failing test. It would also silently skip validation and every other exhaustive visit.
- **Related**: #2697 (`supplemental_texture_indices`, a fourth role walk with no lockstep test — still open), #3465 (the prose-vs-struct parity test, which pins the docs but not `values()`).
- **Suggested Fix**: Add the six-line sibling of `roles_covers_every_field_in_the_set` — count `map_ref`'s visits and assert `values().count()` equals it. That makes all three walks drift-proof and subsumes the sequential-integer test's omission blind spot.


## Per-Dimension Results

Dimensions producing **no findings**: **2 (Geometry/Transform), 3 (Skinning/Lights), 4 (Nodes), 5 (Particles), 6 (Collision), 7 (Animation), 9 (Completeness/cross-cutting)** — seven of nine. Findings came only from Dimension 1 (Material) and Dimension 8 (Shader flags / texture roles).

### Dimension 1 — Material

#### Verified clean

- **`translate_material` signature not widened.** Still `(&ImportedMaterial, Option<&str>, ResolvedPaths, u32) -> Material` (`material_translate.rs:456-461`). The #05d68926 narrowing holds; material translation still provably cannot read geometry.
- **Single boundary.** Exactly three production callers — `scene/nif_loader.rs:959`, `cell_loader/spawn/mesh_instance.rs:634`, `cell_loader/placement_lod.rs:527` (+ `cornell.rs:1994`, the synthetic harness). No second field-by-field `Material` literal in production; every other `Material {` hit is a test or the two declared sibling boundaries (`translate_texture_only_material`, which owns no scalar literals of its own and routes through `resolve_pbr`).
- **PBR resolution.** `metalness`/`roughness` are plain `f32`; no `*_override: Option<f32>` on the canonical type. `resolve_pbr` (`material.rs:1165-1191`) fills only NaN sentinels via `classify_pbr_keyword`, never overwrites an authored BGSM/BGEM override, and clamps to exactly `[0.0, 1.0]` / `[0.04, 1.0]`. No per-draw `classify_pbr` in the renderer — `static_meshes.rs:344-366` reads `m.roughness`/`m.metalness` directly.
- **Glass classified once.** `classify_glass_into_material` is invoked once, from inside `translate_material` (`:590-608`), **after** `resolve_pbr()` so the forced glass roughness wins, and is alpha-aware (`source.has_alpha || source.alpha_test`) with decal and `from_bgsm` provenance gating.
- **Two-phase boundary (#2330).** Both Phase-2 resolvers run at both handle-attaching callers (`scene/nif_loader.rs:1149,1156`; `cell_loader/spawn/mesh_instance.rs:899,907`). `placement_lod.rs` attaches no `MaterialTextureHandles` (grep: zero hits), so its exemption is correct, not an omission.
- **#3530 Oblivion parallax — the newest code in this window, checked hard.** The per-game decision is made exactly once at the NIFAL boundary (`crates/nif/src/import/material/legacy_properties.rs:275-285`, gated on `tex_prop.apply_mode == APPLY_HILIGHT2`), and is deliberately last so an authored parallax slot or a BSShader path wins. It fabricates no constant — it reuses the `4.0` / `0.04` pair every consumer's `unwrap_or` already used. The renderer only *transports* it (`static_meshes.rs:307-311`), correctly gated on `parallax_map_index != 0` so a bare bit cannot make the shader's "is a height map bound" test pass on index 0. Both shader consumers mask it off symmetrically (`include/material_sampling.glsl:49-50` raster, `include/ray_hit.glsl:296-298` RT). `PARALLAX_ALPHA_HEIGHT_BIT` and `NORMAL_ALPHA_SPEC_BIT` are both `0x8000_0000` but on different `GpuMaterial` fields, so no collision, and both are re-exported from `crates/renderer/src/shader_constants_data.rs:448,463` rather than re-declared. **Zero per-game branches downstream.** Clean.
- **`sanitize_finite` top-level coverage.** Mechanically diffed the `Material` struct's 33 float fields against the macro list — all present. #3373's specific hole is closed. (The nested carriers are D1-01.)
- **Authored blend/clamp carried, not re-derived.** `texture_clamp_mode` / `src_blend_mode` / `dst_blend_mode` copied verbatim at `material_translate.rs:584-588`. No render-time re-derivation from raw NIF properties.
- **Emissive scale.** Still a deliberate no-op; `emissive_mult` and `emissive_source` are copied straight across (`:471-472`) with no normalization constant anywhere. Correct per the 2026-08-29 re-census (#3337).
- **`material_kind: u32`** kept as the GPU dispatch contract — not flagged.

#### Stale candidates dropped: 3

1. *`terrain_lod_btr.rs` spawns drawn entities with no canonical `Material`* (carried forward from AUDIT_NIFAL_2026-08-27's cross-audit note as FNV-D2-03). **Fixed** by #3336 — it now calls `translate_texture_only_material` and is the fifth entry in the boundary guard's table. Dropped.
2. *`translate_texture_only_material` is a second, undeclared `Material` construction site.* Disproven: it is an explicitly declared sibling boundary for the three no-source-material populations, owns no scalar literals, and routes through the same `resolve_pbr`. Its one non-default value (`env_map_scale: 0.0`) is documented as load-bearing against `Material::default()`'s 1.0. Not a violation.
3. *`static_meshes.rs:941` / `:1000` construct `Material` in the renderer.* Disproven — both are inside that file's `mod tests`.

### Dimension 2 — Geometry / Transform

#### Verified clean

- **Per-game vertex decode converges.** All three extractors — classic `NiTriShape` (`crates/nif/src/import/mesh/ni_tri_shape.rs`), Skyrim SE+ packed-half `BSTriShape` (`bs_tri_shape.rs`), Starfield `BSGeometry` UDEC3 (`bs_geometry.rs`) — produce the same `Vec<[f32;3]>` positions + `Vec<u32>` indices in renderer space. No `Option`-gated "decode-later" geometry reaches `MeshRegistry::upload`.
- **Z-up → Y-up happens once, at the import boundary, and nowhere else.** `grep` for `zup_point_to_yup` / `zup_matrix_to_yup_quat` across `byroredux/src` and `crates/renderer/src` returns **zero** production hits outside `crates/nif/src/import/`, and a search for any coordinate-frame handling in the renderer (`z_up`/`zup`/`Z-up`) returns **nothing at all**. The consumer is genuinely format-agnostic.
- **Degenerate-rotation SVD repair fires exactly once, at parse.** `sanitize_rotation` has precisely two production call sites, both in `crates/nif/src/stream.rs` (`:742`, `:765` — the two transform readers); every other reference is inside `crates/nif/src/rotation.rs` itself or a test. `compose_transforms` (`crates/nif/src/import/transform.rs:13-25`) does *not* re-check, correctly assuming already-sanitized rotations, so no consumer re-validates raw-tier messiness. The #2456 case (singular-but-not-`is_degenerate_rotation`-visible scaled matrices) is covered by the det branch and pinned by name.
- **Tangents resolve to one array before the vertex buffer.** Authored extra-data first, Mikkelsen synthesis otherwise (`synthesize_tangents` for Z-up inputs, `synthesize_tangents_yup` for already-Y-up `BSTriShape`/`BSGeometry` inputs — one helper pair shared by all three extractors, per the `bs_geometry.rs:299` note). `triangle.vert` reads a single tangent attribute with **no per-game branch**; the zero-length sentinel it honours (`triangle.vert:166`, `:302`) is a numerical guard for degenerate UVs that survived synthesis, not a deferred per-game classification, so it is not a `no-render-time-fallback` violation.
- **`local_bound_radius` is derived at extraction, in Y-up space, by every extractor** (`ni_tri_shape.rs:169`, `bs_tri_shape.rs:143`, `bs_geometry.rs:350`, plus the shared `extract_local_bound` at `types.rs:906`). No render-time bound recomputation anywhere.
- **`canonical_mesh_path` (#2361 / #3391)** remains parse-side and byte-neutral; not a translation concern.

#### Stale candidates dropped: 0

### Dimension 3 — Skinning & Lights

#### Verified clean

##### Skinning
- **`ImportedSkin` emits global bone indices.** The #613 partition-local → global remap is still done at extraction, and the defensive guard is intact: `crates/nif/src/import/mesh/skin.rs:250-255` warns when `bone_refs_slice.len() > u16::MAX` so the truncation gap surfaces in test runs rather than silently aliasing. (Pre-#613 this silently aliased every vertex past partition 0 — SK-D1-01.)
- **`global_skin_transform` is carried through** and converted to Y-up once, at extraction (`skin.rs:155`, `:264`, via `ni_transform_to_yup_matrix`).
- **Palette skinning stays game-agnostic downstream** — no consumer re-derives partition layout.
- **#2440 unchanged (documented gap, not re-reported).** `SkinnedMesh::new_with_global` still has exactly **one** production producer, `byroredux/src/scene/nif_loader.rs:1234`; every other hit (`systems/bounds.rs:425,449,474,503`, `render/skinned.rs:347`, `vulkan/skin_compute.rs:1607`) is inside a `#[cfg(test)] mod tests` block — verified by locating each file's test-module boundary, not assumed. So cell-placed skinned geometry (Skyrim/FO4 wind-animated cloth, banners, chains) still spawns with weights uploaded but no palette binding, rendering in bind pose. Unchanged since 2026-08-07; closing it needs the cell loader to grow a per-placement node-entity map.
- **#2441 residual re-confirmed as a terminal sentinel, not a resolve-later leak.** `SkinnedMesh.bones` / `skeleton_root` carry `Option`s that `compute_palette_into` substitutes `Mat4::IDENTITY` for; the producer logs the unresolved-bone warning. Recorded, not re-filed.
- #3355 / #3360 (SSE `SkinPartition` triangles are global indices) are parse-side index-space fixes, correctly out of scope here.

##### Lights
- **The raw block-type discriminator collapses at translate and never reaches the renderer.** `NiAmbientLight` / `NiDirectionalLight` / `NiPointLight` / `NiSpotLight` appear in `byroredux/src/render/` and `crates/renderer/src/` **only** inside explanatory comments (`render/lights.rs:799-802`); every runtime read is of the canonical `LightKind`. No downstream `match` on source block type.
- **`LightKind` lives on the canonical `LightSource` component** (`crates/core/src/ecs/components/light.rs`); `byroredux_nif::import::LightKind` is a re-export of the same type, not a second copy — so the canonical-type rule ("promote the ECS component, don't add a third type") is satisfied without a parallel struct.
- **The #2205 → #2439 chain holds.** `translate_light` remains the single boundary all three ESM-LIGH producers collapse onto, `LIGHT_FLAG_SPOT` (0x200) stays distinct from `LIGHT_FLAG_SHADOW_SPOTLIGHT` (0x400), and the `world_direction` rotation at `cell_loader/spawn.rs` is intact (re-verified, as in the 08-23 and 08-27 sweeps).

#### Stale candidates dropped: 1

1. *`SkinnedMesh::new_with_global` has six producers, so skinning has a `single-boundary` violation.* Disproven — five of the six are inside test modules. Checking each file's `#[cfg(test)]` boundary (rather than trusting the grep count) collapses it to the one known production producer, which is #2440's documented state.

### Dimension 4 — Nodes

#### Verified clean

- **The seven parked fields still reach no canonical component.** I grepped each field across `byroredux/src` and `crates/`, excluding the raw-tier declaration (`crates/nif/src/import/types.rs`), the parser (`crates/nif/src/blocks`), the import walk, tests, and `: None` constructions. Results:

  | Field | Non-excluded hits | What they are |
  |---|---|---|
  | `bs_value_node` | 1 | `crates/spt/src/import/mod.rs:872` — a SpeedTree test *asserting it is `None`* |
  | `bs_ordered_node` | 1 | `crates/spt/src/import/mod.rs:873` — same |
  | `tree_bones` | 1 | `crates/spt/src/import/mod.rs:874` — same |
  | `range_kind` | 1 | `crates/spt/src/import/mod.rs:875` — same |
  | `lod_group` | 0 | — |
  | `bs_lod_cutoffs` | 1 | `crates/nif/src/import/mesh/bs_tri_shape.rs:258` — the raw-tier *population* site |
  | `bs_sub_index` | 3 | `bs_tri_shape.rs:262` (population) + `crates/nif/examples/bto_segment_census.rs:54,89` (a diagnostic example binary, not engine code) |

  Not one of these is a canonical ECS consumer. The gap remains bounded and recorded, exactly as `docs/engine/nifal.md` §2 states.
- **Live node data is consumed on both paths.** `name`, `flags` → `SceneFlags` (`scene/nif_loader.rs:563`, `:1062`), `collision` → `CollisionShape`/`RigidBodyData`, and `billboard_mode` → `Billboard`. No canonical node field is dropped.
- **#2206 stays fixed on the cell path** — the half-stale claim that burned four prior sweeps. The flat walk's per-mesh billboard sibling is attached at `cell_loader/spawn/mesh_instance.rs:794-795` and the placement-root mode at `cell_loader/spawn.rs:859`, alongside the loose-NIF path's `scene/nif_loader.rs:548`/`:1076`. Verified in code, not restated from the spec's prose.
- **No `translate_node` boundary, correctly.** The absence is by design (the two load paths handle nodes structurally differently — full NiNode hierarchy vs. flattened placement-root) and documented in spec §2. Not flagged as a `single-boundary` violation.
- **`NiTextureEffect` remains genuinely dead and content-absent.** `import_nif_texture_effects` has **zero** production call sites — every hit is in `crates/nif/src/import/walk/tests.rs`. That matches the spec's measured 0 occurrences across Oblivion / FNV / Skyrim mesh archives; the extractor is dead because there is nothing to consume, so building a projector pass would be speculative work. Correctly not a leak.
- Documented passthroughs from spec §2 (`BSInvMarker` parsed-not-walked, `NiSwitchNode` identity walked via active-index, `bs_bound` loose-path-only) unchanged; not re-reported.

#### Open Dimension-4 issues seen and deliberately not re-reported
- **#3072** — `finish_partial_import` hardcodes `furniture: None`, and the process-lifetime NIF cache preserves it. OPEN.
- **#3074** — the stated blocker for dropping `flame_attach_offset` on the streaming path is false. OPEN.

#### Stale candidates dropped: 0

### Dimension 5 — Particles

#### Verified clean

- **`apply_emitter_overlays` is the single overlay site, and both load paths route through it** — `byroredux/src/scene/nif_loader.rs:610` and `byroredux/src/cell_loader/spawn.rs:1061`, both via `crate::systems::apply_emitter_overlays`. No inline overlay at either spawn site. (Multiple callers of one boundary is correct, not a violation.)
- **The boundary has absorbed the two override families that used to sit outside it.** This is the #1513 dedup working as intended rather than eroding:
  - **#2300** folded in `texture_path` / `src_blend` / `dst_blend`, which had been copy-pasted outside the boundary at *both* load sites (`particle.rs:100-112`) — precisely the divergence shape the boundary exists to prevent, caught and collapsed.
  - **#3344** folded in the authored `BS Max Vertices` budget (`:117-131`), which the parser read, documented, and then dropped in favour of the preset's name-heuristic guess. It clamps to `MAX_PARTICLES_CEILING` and **logs the clamp**, so the truncation is visible rather than silent — a bounded engine limit, not a fabricated value.
- **`initial_color` is still deliberately NOT applied.** The contract is stated at `particle.rs:25` and `:60` and pinned by two tests that assert the white nif.xml default "must NOT win" over the tuned preset (`:591`, `:633`, and `:617`'s named case). Colour stays owned by the `color_curve` override. No reverse `no-fabrication` regression.
- **Authored kinematic + lifetime fields override the preset** via `apply_emitter_params` (speed, speed_variation, declination, declination_variation, life, life_variation).
- **Particle size: the authored magnitude contract holds.** `size = p.initial_radius * p.base_scale.unwrap_or(1.0)` (`particle.rs:39`), with `start_size_variation = radius_variation.abs() * base_scale` (`:45`, #1775). `base_scale` is still applied — a change "restoring" the withdrawn pre-#2488 doc claim would regress FNV oasis smoke to ~7× oversized. The grow→steady→fade bell shape remains deliberately untranslated (documented future work, not a leak).
- **Spawn rate is authored and sentinel-guarded.** `extract_emitter_rate` (`crates/nif/src/import/walk/mod.rs:916`) gates every candidate through `sane()`: `r.is_finite() && 0.0 < r && r < 3.0e38` (`:930`), which rejects NaN, ±Inf, negatives and the nif.xml FLT_MAX sentinel — the #1363/#1364 pin, intact. The same `< 3.0e38` sentinel test guards the colour path at `:735-740`. Legacy `NiParticleSystemController` content with no controller keeps the preset rate.
- **Force fields are converted Z-up→Y-up once, at overlay time** (`convert_force_fields_zup_to_yup`, `particle.rs:139-...`), delegating the swap to `byroredux_core::math::coord::zup_to_yup_pos` (the #1617 single source of truth) rather than an inline `[x, z, -y]`. Not per-particle, not per-frame.
- **Starfield particle slice N/A** (#2354) — structurally unreachable, pinned by `starfield_corpus_has_no_particle_blocks`. Unchanged, not re-reported.

#### Stale candidates dropped: 1

1. *#2610 — particle `DrawCommand.effect_shader_flags` is still hardcoded `0` at `byroredux/src/render/particles.rs`.* **Stale**: carried in the 2026-08-27 report's documented-limitation ledger, but the issue was closed by commit `70f1bb74` ("BGEM particle effect flags"). The renderer now forwards the authored word — `effect_shader_flags: em.effect_shader_flags` (`particles.rs:281`) — packed by `cell_loader::pack_effect_shader_flags`, with two tests covering the authored and unauthored cases (`:365`, `:388`). Dropped, and flagged for removal from the carried-forward ledger.

### Dimension 6 — Collision

#### Verified clean

- **Shape coverage is 16/16, mechanically diffed.** `grep -oE "Bhk[A-Za-z]*Shape\b" crates/nif/src/blocks/mod.rs | sort -u` and the `downcast_ref::<Bhk…>` arm list from `crates/nif/src/import/collision/shape.rs` are **byte-identical sets**: `BhkBoxShape`, `BhkCapsuleShape`, `BhkCompressedMeshShape`, `BhkConvexListShape`, `BhkConvexSweepShape`, `BhkConvexVerticesShape`, `BhkCylinderShape`, `BhkListShape`, `BhkMeshShape`, `BhkMoppBvTreeShape`, `BhkMultiSphereShape`, `BhkNiTriStripsShape`, `BhkPackedNiTriStripsShape`, `BhkPlaneShape`, `BhkSphereShape`, `BhkTransformShape`. No dispatched shape falls through to the unsupported-shape fallback.
- **The automated guard is real and correctly scoped.** `dispatch_coverage_tests` (`crates/nif/src/import/collision/mod.rs:598-660`) derives the dispatched set by scanning `blocks/mod.rs` for arms whose *match key* is a quoted `"bhk…Shape"` — which correctly excludes `…ShapeData`, `…Phantom`, collision objects and constraints — and handles the two-line `bhkTransformShape | bhkConvexTransformShape` alias arm by probing the following lines. It is a genuine structural guard, not a hardcoded count, so a newly-dispatched shape without a resolve arm fails the build rather than silently dropping collision. I did not need to extend it.
- **`BhkPlaneShape` remains the single deliberate `None` arm** (#1334), documented at its arm, with the trimesh fallback covering the ground surface. Not a leak.
- **The #9c6096aa / #1360 / #1361 regression pins hold**: `BhkMultiSphereShape` → `Compound` of `Ball` children (single centred sphere unwraps to a plain `Ball`); `BhkConvexListShape` → `Compound` of resolved convex sub-shapes; `BhkConvexSweepShape` delegates to its inner `shape_ref`; `BhkMeshShape` resolves tri-strip data with per-axis scale. All four still present as live arms.
- **`havok_scale` is applied exactly once and never re-applied.** It is derived at parse from the header version triplet (`havok_scale_for`, `crates/nif/src/lib.rs:92`) — wire format, not a runtime game switch — and carried on `NifScene.havok_scale`. Grepping every reference outside `crates/nif/src/import/collision/` and the parser finds **no consumer that re-applies it**; the single hit in `byroredux/src/cell_loader/spawn/mesh_instance.rs:995` is a comment explaining that verts already have it baked in at extract time.
- **`hkMotionType` collapses fully at translate** (`collision/mod.rs:224-229`): `1..=5 | 8 => Dynamic`, `6 => Keyframed`, `7 => Static`, `9 => CharacterKinematic`, `0`/out-of-range `=> Static`, each pinned by a named test (`:788-801`). This is the correct canonical enum, **not** the old `4 => Keyframed / _ => Static` `no-fabrication` regression. Every downstream consumer I traced (`npc_spawn.rs:257`, `systems/cinematic.rs:237`, `ragdoll.rs`, `save_io.rs`, `scene/nif_loader.rs:537`) reads `RigidBodyData.motion_type` — the canonical enum — and none inspects a raw Havok byte.
- **The #1832 mass-0 reclassification survives** (`collision/mod.rs:407-414`): a `Dynamic` body with `mass <= 0.0` is reclassified `Static`, which is what keeps Skyrim architecture solid.
- **`CollisionAuthoringSummary` crosses the boundary clean.** Exactly three `u32` counts (`classic` / `new_physics` / `phantom`, `collision/mod.rs:88-92`) — no `bsver`, no raw block-type string, no per-game enum. `needs_packed_havok_fallback()` is `new_physics > 0`. `classify_collision_block` does the per-game discrimination on the *raw* side via `is::<BhkCollisionObject>` / `is::<BhkNPCollisionObject>` / `is::<BhkPCollisionObject>`, so the discriminator collapses before it rides `CachedNifImport.collision_authoring`.
- **The packed-Havok proxy stays renderer-free.** `spawn_packed_havok_proxy` (`byroredux/src/cell_loader/spawn.rs:250-294`) inserts `Transform`, `GlobalTransform`, the `CollisionShape`, `RigidBodyData`, `Parent(placement_root)`, `PhysicsSourceForm` and the render layer — and **no `MeshHandle`**. A blob-derived guess therefore cannot enter the BLAS/TLAS as if it were authored geometry.
- **TriMesh / primitive validity guards intact** — non-finite centre/radius (`shape.rs:118`), vertex arrays (`:188`, `:715`), transform translation/rotation (`:265`, `:773`) and zero/non-finite scale components (`:341`) all reject rather than propagate, so the cell loader's synthesized fallback stays available.
- **Documented limitations stay documented, and stay limitations.** The table at `crates/nif/src/import/collision/mod.rs:11-12` still records `BhkNPCollisionObject` as **approximated** (blob opaque; census-selected render-geometry proxy) and `BhkPCollisionObject` as **not modelled** (phantoms need a `TriggerVolume` ECS path, not a rigid body), including the FO3-DLC `bhkSPCollisionObject` alias (#2332). Not re-reported.

#### Stale candidates dropped: 1

1. *`BhkSimpleShape` is dispatched in `blocks/mod.rs` but has no resolve arm — a 17th type silently dropping authored collision.* **My own false positive, and worth recording as the trap it is.** The greedy pattern `Bhk[A-Za-z]*Shape` matched the *prefix* of `BhkSimpleShapePhantom`, which is a phantom, not a shape — and it is in fact explicitly handled at `shape.rs:315` alongside `BhkAabbPhantom`. Re-running with a word boundary (`Bhk[A-Za-z]*Shape\b`) collapses the set back to 16, identical to the resolve-arm set. This is exactly why the checked-in `dispatch_coverage_tests` keys on the *quoted match string* rather than the struct identifier: an auditor's ad-hoc grep gets this wrong and the committed guard does not.

### Dimension 7 — Animation / controllers

#### Verified clean

- **`convert_nif_clip` is the single NIF→core `AnimationClip` boundary.** Seven production callers — `scene.rs:1024`, `npc_spawn.rs:518`, `scene/nif_loader.rs:1367`, `systems/animation.rs:1864`, `cell_loader/partial.rs:89`, `cell_loader/references/synth_child.rs:573` — all route through the one function. Multiple callers of one boundary is correct, not a `single-boundary` violation.
- **Scalar sanitisation at the boundary has *grown* since the last sweep, in the right direction.** `frequency` is resolved through `sanitized_clip_frequency` (#3258) and `phase` is now finiteness-gated too (#3345, `anim_convert.rs:521-532`) — the latter added because `phase` seeds `AnimationPlayer::local_time` and a NaN would latch the player on the first tick. Both reject non-finite at the translate boundary "the same way every other per-game quirk is", which is exactly the tier discipline. `frequency` additionally rejects non-positive rates with a stated rationale (backwards playback is `CycleType::Reverse`, not a negative frequency) rather than a silent clamp.
- **`duration` / `weight` remain raw — already filed, not re-reported.** `anim_convert.rs:507` (`duration: nif.duration`) and `:533` (`weight: nif.weight`) still copy unvalidated, three lines from their now-sanitised siblings. This is **#3432** (SAFE-2026-08-27b-01, OPEN, `medium`, labelled `animation`/`safety`). Correctly owned by the safety report; recorded here so the next NIFAL sweep does not rediscover it.
- **Per-game variation is resolved at import, not downstream.** B-spline compressed interpolators sampled to linear keys (reachable on FO3/FNV as well as Skyrim+ — the `feedback_bspline_not_skyrim_only` rule, respected in the code), XYZ-Euler rotation keys composed to quaternions, TBC/Hermite tangents decoded, Z-up→Y-up once. Player/stack consumers see only game-agnostic quaternion keys.
- **Text-key events wired**; embedded controllers set `text_keys: Vec::new()` at exactly one site (`crates/nif/src/anim/entry.rs:303`) — a deliberate empty for mesh-local controllers that carry no event keys, not a drop.
- **`color_target_from_target_color`** (`crates/nif/src/anim/channel.rs:338-345`) is shared by the KF arm and the embedded-controller arm specifically so a future `ColorTarget` variant cannot silently diverge between the two import paths (#2304 / NIFAL-D7-03). The right shape.
- **The material-ambient colour channel is live, not parked** — `ColorTarget::Ambient` → `AnimatedAmbientColor` (`anim_convert.rs:173`) → applied by `systems/animation.rs:201` → read by the renderer at `render/static_meshes.rs:127`. Distinct from the spec's parked *per-light* ambient note; no contradiction, but worth recording so a future sweep doesn't read the two as the same channel.
- **`convert_hkx_clip` is the correctly-declared second boundary** (`byroredux/src/asset_provider/animation.rs:165`) for Skyrim's Havok-packfile cart/furniture idles — same source-agnostic `AnimationClip` target, no parallel struct (#2305). Its one `no-fabrication` exception, `behavior_completion_events` synthesising `ExitCartEnd` / `IdleFurnitureExit`, is explicitly documented and justified (those completions live in Skyrim's behavior graph, not the per-clip `.hkx`), is gated on the authored event-name pattern, and only fires when neither event is already present in the authored annotations. It also drives `cycle_type` consistently (`:282-286`). A declared, bounded exception — not a leak.
- **Raw-tier `byroredux_nif::anim::AnimationClip`** remains a distinct, type-qualified parse-tier struct that never reaches the ECS unconverted (#2442). Doc-precision item, already resolved.

#### Stale candidates dropped: 1

1. *`frequency` is the only sanitised scalar at the clip boundary; `phase` is still raw.* Stale as of `d1bcf6e2` (#3345) — `phase` is now finiteness-gated at `anim_convert.rs:521-532` with its rationale inline. Dropped.

### Dimension 8 — Shader flags / texture roles

#### Verified clean

- **Zero per-game branches in the shaders — the cardinal check for this dimension.** Scanned every `*.frag` / `*.vert` / `*.comp` and every `include/*.glsl` for game names, `bsver`, and version-as-behaviour switches. All hits are explanatory comments (`composite.frag:477`, `lighting.glsl:88-109`, `water.frag:95-107`, …). No runtime per-game branch anywhere in the shader tree.
- **Per-game slot vocabulary stops at the import boundary.** `TextureSlotLayout` is derived from the wire format, not a runtime game switch — `TextureSlotLayout::from_bsver` (`slot_role.rs:104-116`). It rides the **raw** `ImportedMaterial.texture_slot_layout` (`types.rs:642`), which the tier model permits, and the canonical `Material` has no such field (verified against its full field list). Correct shape.
- **`slot_to_role` is one decision tree keyed on `(layout, slot)` + block-derived context**, and essentially every arm cites a measured corpus count with its issue number (#2694 3158/3158 FaceTint `*_sk.dds`; #2997 31,303 FO4 slot-3 properties; #2999 n=1,229 FO4 slot 4/5 occupancy; #3085 1,616/1,664 FO76 `_s.dds`; #2693 607/607 type-11 slot 6). No guessed heuristics. `unrouted_texture_slot_bindings` counts non-empty bindings that reached no role, per layout and slot, so future table gaps stay observable instead of vanishing.
- **`smooth_spec` vs `specular` are not merged** — the likeliest mis-merge, checked directly. `specular` is produced only by `slot_to_role`'s `TextureRole::Specular` arms (Skyrim/Starfield slot 7 MSN, FO4 slot 7, FO76 slot 6); `smooth_spec` only from the legacy gloss map (`material/mod.rs:1247`) and BGSM's `smooth_spec_texture` (`asset_provider/material.rs:1399-1400`). Disjoint producers, and `types.rs:323` documents the distinction on the field itself. Same for `environment` vs `environment_mask` (distinct arms at slots 4 and 5 on every layout).
- **#3458 (the 08-27 HIGH) stayed fixed and is correctly sourced.** `slot_to_colocated_role` cites nif.xml's own slot description and bit-25 name (`nif.xml:6434`) plus the #2694/#3068 measurements — a sourced position, not an inference. (Its one un-propagated consumer is D8-01.)
- **#1592 FO4 render-affecting flags still reach `MaterialInfo`.** `Model_Space_Normals` via `slsf1_bit(slot_layout, skyrim_slsf1::MODEL_SPACE_NORMALS, fo4_slsf1::MODEL_SPACE_NORMALS)` **plus** the FO76+ `MODELSPACENORMALS` CRC over both `sf1_crcs` and `sf2_crcs` (`dedicated_shader.rs:158-169`); `Alpha_Test` via `fo4_slsf2::ALPHA_TEST` (`:360-364`), with the `alpha_threshold == 0.0` gate that #2091 established so it never overrides authored intent. Neither is dropped.
- **All 6 `BSLightingShaderProperty` trailing-data families reach the canonical tier** — `ShaderTypeFields` (`crates/core/src/ecs/components/material.rs:467-481`) carries SkinTint, HairTint, Eye, Parallax, MultiLayer and Sparkle payloads, and `translate_material:499-503` forwards the whole box. The pre-#343 8-of-9 drop stays closed.
- **`map_ref` / `values()` / `secondary_values()` role parity** — 22 named + 4 decals = 26 everywhere, same order, `base_color` first. `documented_texture_role_list_matches_the_struct` (#3465) additionally pins the count *and every role name* into both `docs/engine/nifal.md` and this skill file, derived from a source scan rather than a hardcoded number.
- **`ShaderFlags<'a>` stays deleted** (#1897); production reads the namespaced constants through `is_decal_from_legacy_shader_flags` / `is_decal_from_modern_shader_flags` / `is_two_sided_from_modern_shader_flags`.
- **`EmissiveSource::Effect` / `material_kind == 101`** still tag the `base_color_scale` diffuse-tint deferral rather than dropping it. Not re-reported.

#### Stale candidates dropped: 2

1. *`roles()` (added #3349 in this window) is a new unprotected hand-written role walk.* Disproven — `roles_covers_every_field_in_the_set` (`types.rs:1721-1734`) cross-checks it against `map_ref`'s compiler-protected visit count, which is a genuinely drift-proof guard. Checking it is what surfaced that `values()` lacks the same guard (D8-02).
2. *The FO76 slot-6 → `Specular` arm collides with Skyrim's slot-6 → `InnerLayer`.* Disproven — the two are separate `(layout, slot)` match arms, and the FO76 divergence is measured (#3085: 1,616 of 1,664 bindings are `_s.dds` across 95,041 FO76 NIFs), not assumed.

### Dimension 9 — Completeness + cross-cutting

#### Verified clean

- **`no-render-time-fallback` holds end to end — the strongest result of this sweep.** Two independent scans:
  - **GLSL**: every `*.frag` / `*.vert` / `*.comp` and every `include/*.glsl` scanned for game names, `bsver`, and version-as-behaviour switches. Every hit is an explanatory comment; **zero** runtime per-game branches.
  - **Rust**: `grep -rniE "GameKind::|game ==" crates/renderer/src byroredux/src/render/` returns **nothing**. The renderer crate and the render-data-collection module contain no per-game branch at all.
  Per-game divergence is genuinely translated away before the consumer tier, in both languages.
- **`single-boundary` — every declared category has exactly one boundary, and each one's call sites check out.**

  | Category | Boundary | Production call sites |
  |---|---|---|
  | Material | `translate_material` | 3 (`scene/nif_loader.rs:959`, `cell_loader/spawn/mesh_instance.rs:634`, `cell_loader/placement_lod.rs:527`) |
  | Material (no source record) | `translate_texture_only_material` | 4 exterior spawners, pinned by `every_exterior_spawner_inserts_a_boundary_material` |
  | Material markers | `attach_blend_and_facing_markers` | 2 (`scene/nif_loader.rs:1017`, `cell_loader/spawn/mesh_instance.rs:911`), pinned by a source-scan guard at `material_translate.rs:1072` |
  | Mesh water | `attach_mesh_water` | 2 (`scene/nif_loader.rs:1115`, `cell_loader/spawn/mesh_instance.rs:864`) |
  | Lights (ESM) | `translate_light` | 3, all in `cell_loader/references/synth_child.rs` (`:295`, `:401`, `:645`) |
  | Particles | `apply_emitter_overlays` | 2 (`scene/nif_loader.rs:610`, `cell_loader/spawn.rs:1061`) |
  | Animation (NIF) | `convert_nif_clip` | 7 |
  | Animation (HKX) | `convert_hkx_clip` | declared second boundary, same canonical target |
  | Nodes | — | N/A by design (spec §2) |

  Multiple callers of one boundary is the correct shape throughout; I found no second field-by-field construction site for any canonical type.
- **`translate_light(ld, game, ref_rot)` taking a `GameKind` is correct, not a violation** — worth stating explicitly because it looks like one at a glance. The per-game parameter sits *at* the translate boundary, which is exactly where per-game divergence is required to be resolved; the rule forbids per-game branches *downstream* of it, and the scans above confirm none exist.
- **`no-fabrication` holds.** The two canonical "measured, then deliberately NOT normalized" examples are intact: the emissive no-op (re-censused 2026-08-29 over 196,794 files, #3337 — no source is offset from the others by a fixed factor, so there is no constant to apply) and the particle colour / size-over-life deferrals. Every new constant I encountered cites a measurement and an issue number — the `slot_to_role` table is the strongest example, with per-arm corpus counts (#2694, #2997, #2999, #3085, #2693). The #3530 Oblivion parallax work notably *declined* to invent a game-specific height-scale constant and reused the existing engine default instead.
- **The completeness harness's known limits are already filed, not re-reported**: **#2532** (the canonical-tier harness covers 1 of ~5 declared translate boundaries) and **#3462** (its "reverting any single `source.X` line fails an assertion" contract is false for four fields, two of which gate the NIFAL↔WATAL seam). Both OPEN. `cross_game_translation_completeness` (`crates/nif/tests/translation_completeness.rs:340`) remains `#[ignore]`-gated on real game data, so it is a manual per-game signal rather than a CI gate — as designed.

#### Note on execution
Per the memory constraint in force for this run, I did **not** invoke cargo. Every result above is from static analysis — reading, grepping and mechanically diffing the live tree — which is what all four tier invariants are actually checkable by. The one thing this forecloses is running the `#[ignore]`d per-game fill-rate harness, whose output would have been a *lead* rather than gospel in any case (its own charter says to verify the underlying extractor).

#### Stale candidates dropped: 0

## Documented-Limitation Ledger (re-verified this cycle, not re-reported)

These are `parked-not-leak` or already-filed. Restated so the next sweep does not re-derive them.

- **#3073** — `parallax_height_scale` / `parallax_max_passes` still bypass `translate_material`: raw `Option<f32>` on `ImportedMaterial` resolved by `.unwrap_or(0.04)` / `.unwrap_or(4.0)` at the spawn sites plus a per-draw copy at `byroredux/src/render/static_meshes.rs:315-320`. OPEN, unchanged. Note the interaction with #3530: the Oblivion `APPLY_HILIGHT2` arm now *seeds* both fields at the boundary (`legacy_properties.rs:279-284`), so the duplicated defaults are increasingly the only unconverged half.
- **#3432** (SAFE-2026-08-27b-01) — `AnimationClip.duration` / `.weight` cross `convert_nif_clip` unsanitised while `frequency` (#3258) and now `phase` (#3345) are resolved at the boundary. OPEN, owned by the safety report.
- **#2440** — cell-placed skinned geometry renders in bind pose; `scene/nif_loader.rs:1234` is still the only production `SkinnedMesh::new_with_global`. Unchanged.
- **#2441** — `SkinnedMesh.bones` / `skeleton_root` `Option`s are a terminal "bone lookup failed" sentinel logged at the producer, not a resolve-later leak. Recorded, not re-filed.
- **Node/mesh passthroughs** — `bs_value_node`, `bs_ordered_node`, `tree_bones`, `range_kind`, `lod_group`, `bs_lod_cutoffs`, `bs_sub_index`: re-grepped field by field this sweep, all seven still have **zero** canonical ECS consumers. Every non-excluded hit is a SpeedTree test asserting `is_none()`, the raw-tier population site, or a diagnostic example binary.
- **`BhkNPCollisionObject` / `BhkPCollisionObject`** — FO4+ packed-Havok blob (approximated via the authoring-aware `CollisionAuthoringSummary` proxy) and Skyrim+ phantoms (need a `TriggerVolume` ECS path). Both still documented in the table at `crates/nif/src/import/collision/mod.rs:11-12`.
- **`BhkPlaneShape`** — still the one deliberate `None` arm of the 16 (#1334), documented at its arm.
- **`HkPackedNiTriStripsData.sub_parts`** (#2550) — decoded, deliberately parked with a documented unblocking consumer. Zero import consumers, correctly.
- **`NiTextureEffect`** — `import_nif_texture_effects` still has **zero** production call sites, and the block is content-absent across Oblivion/FNV/Skyrim. Dead because there is nothing to consume; do not build speculatively.
- **`NiLODNode` / `lod_group`** — content-absent, forward-compat foundation only.
- **`BSInvMarker` / `NiSwitchNode` identity / `bs_bound` cell-path** — documented passthroughs, unchanged.
- **Starfield particle slice N/A** (#2354) — pinned by `starfield_corpus_has_no_particle_blocks`.
- **#2327 / SKY-D7-02** — SLSF1 `Refraction` without `Fire_Refraction` has no canonical field or shader consumer; deliberate, since nif.xml states Refraction Strength is "not based on physically accurate refractive index" and so cannot ride `ior`.
- **#3187** — `RefrTextureOverlay::apply_slot_swap` remains a third slot table. OPEN. (D8-01 is a *distinct* defect on the same overlay path — the `pick` closure's inability to see colocated roles — not a restatement of this one.)
- **#2697** — `supplemental_texture_indices`, a fourth hand-written role walk with no lockstep test. OPEN, and the sibling of D8-02.
- **#2532 / #3462** — the canonical-completeness harness covers 1 of ~5 declared boundaries, and its "reverting any single `source.X` line fails an assertion" contract is false for four fields. Both OPEN.
- **#3072 / #3074** — Dimension-4 items (`finish_partial_import` hardcodes `furniture: None`; the stated `flame_attach_offset` blocker is false). Both OPEN.

## Ledger corrections — previously-carried items now FIXED

Two entries in the 2026-08-27 report's ledger are stale and should be dropped from future carry-forward:

- **#2610** — particle `DrawCommand.effect_shader_flags` is no longer hardcoded `0`. Closed by `70f1bb74`; `byroredux/src/render/particles.rs:281` forwards `em.effect_shader_flags`, with tests for the authored and unauthored cases.
- **FNV-D2-03** — `terrain_lod_btr.rs` no longer spawns drawn entities without a canonical `Material`. Closed by `#3336`; it routes through `translate_texture_only_material` and is the fifth entry in `every_exterior_spawner_inserts_a_boundary_material`.

## Stale Candidates Dropped: 8

Per the standing rule that roughly one finding in six in past sweeps was stale, every candidate was re-checked against current code before inclusion. Eight were dropped:

| # | Candidate | Why dropped |
|---|---|---|
| 1 | `terrain_lod_btr.rs` spawns drawn entities with no canonical `Material` | Fixed by #3336; now in the boundary guard's table |
| 2 | `translate_texture_only_material` is an undeclared second `Material` site | Explicitly declared sibling boundary; owns no scalar literals; routes through `resolve_pbr` |
| 3 | `static_meshes.rs:941`/`:1000` construct `Material` in the renderer | Both inside `mod tests` |
| 4 | `SkinnedMesh::new_with_global` has six producers | Five are in test modules; one production producer (#2440's known state) |
| 5 | #2610 particle `effect_shader_flags` hardcoded `0` | Fixed by `70f1bb74` |
| 6 | `BhkSimpleShape` is a 17th dispatched shape with no resolve arm | **My own greedy-grep artefact** — matched the prefix of `BhkSimpleShapePhantom`, which is a phantom (handled at `shape.rs:315`), not a shape. With a word boundary the set is 16, identical to the resolve arms |
| 7 | `roles()` is a new unprotected hand-written role walk | `roles_covers_every_field_in_the_set` cross-checks it against `map_ref`'s compiler-protected visit count — a genuinely drift-proof guard |
| 8 | `phase` is unsanitised at the clip boundary | Fixed by #3345 (`d1bcf6e2`) |

Candidate 6 is the instructive one: an auditor's ad-hoc grep got the collision
coverage question wrong in the direction of a false HIGH, and the *committed*
`dispatch_coverage_tests` guard — which keys on the quoted match string rather than
the struct identifier — gets it right. That is an argument for extending the checked-in
guard rather than re-deriving its cross-check by hand, exactly as the charter says.

## Verification Method

- Read `docs/engine/nifal.md` in full plus `AUDIT_NIFAL_2026-08-27.md` before touching code; confirmed each of its five findings is closed by `d5a8c36c`.
- Enumerated the delta window `d5a8c36c..HEAD` (40 commits) restricted to every NIFAL-relevant path, and prioritised the newest code — `19813460` (#3530 Oblivion parallax, landed 2026-08-29) got the deepest read on the grounds that it is the least-reviewed change touching the boundary.
- Mechanically diffed, rather than eyeballed, the three list-parity questions this layer keeps regressing on: `Material`'s float fields vs `sanitize_finite`'s macro list (33/33 at the top level — that is what surfaced D1-01's two nested carriers); `MaterialTextureSet`'s struct fields vs `values()` / `roles()` / `map_ref` (22 named + 4 decals = 26, consistent); and `blocks/mod.rs`'s dispatched `bhk*Shape` set vs `shape.rs`'s `downcast_ref` arms (byte-identical, 16/16).
- Traced each finding's data path end to end to its consumer before filing it — `effect_falloff` and `shader_type_fields` to `GpuMaterial` and the material-table hash; the water spawn sites through `render/water.rs`'s re-emit into `collect_static_mesh_draws`' no-`Material` arm; `slot_to_colocated_role` to its single call site.
- Scanned for the cardinal violation in both languages: GLSL (`*.frag`/`*.vert`/`*.comp` + `include/*.glsl`) and Rust (`crates/renderer/src`, `byroredux/src/render/`). Zero per-game runtime branches in either.
- Checked every finding against `/tmp/audit/issues.json` (160 open issues) and the prior report's ledger before filing.

Suggested follow-up: `/audit-publish docs/audits/AUDIT_NIFAL_2026-08-30.md`
(domain label `nifal`, plus `renderer`/`shaders` for D8-01's overlay half and
`save-load` for D1-01's gate half; `game:skyrim` on D8-01.)
