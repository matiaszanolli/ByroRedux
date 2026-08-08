# Legacy Compatibility Audit — Comprehensive Sweep — 2026-08-07

**Base:** `79bfc76e` · **Type:** full comprehensive `/audit-legacy-compat` sweep (7 dimensions)

**Supersedes** the same-day narrow "Physics Closeout" report that previously occupied this
path. That closeout's 4 remediated findings are carried forward verbatim in
[Remediated Earlier Today](#remediated-earlier-today) below and were independently
re-verified as still holding by this sweep's PHYSAL dimension — nothing regressed.

**Scope:** All 7 dimensions of `/audit-legacy-compat`: coordinate-system correctness
(Z-up→Y-up), NIFAL cross-layer mapping shape, the material translation boundary (NIFAL
reference slice), PHYSAL (per-game Havok → solver), EXAL (exterior environment), per-game
translation-survey patterns (Pattern A/B/C upstream branches), and subsystem coverage vs
legacy Gamebryo fidelity. Each dimension was run as an independent sub-agent pass against
live source, cross-checked against `docs/engine/{nifal,exal,physal,per-game-translation-survey}.md`
and deduplicated against the 94 open issues fetched via `gh issue list`.

**Method:** Each dimension traced its claimed single-boundary contract to every call site,
attempted to disprove each candidate finding against current source before keeping it, and
checked open-issue titles for keyword overlap before filing as NEW. No GitHub issue state
was mutated by this audit.

## Executive Summary

| Severity | Count | Notes |
|---|---:|---|
| CRITICAL | 0 | — |
| HIGH | 0 | — |
| MEDIUM | 14 | 13 NEW + 1 scope-correction on existing #2221 |
| LOW | 15 | 13 NEW + 2 scope-corrections on existing #2371/#2372 |
| **Total findings** | **29** | 27 NEW, 2 Existing (confirmed/scoped), plus 4 remediated earlier today |

No CRITICAL or HIGH findings. This reflects that the three canonical-translation
boundaries (NIFAL/EXAL/PHYSAL) are structurally intact — every finding below is either a
**consumer gap** on already-parsed data, an **undocumented `Option`/second-producer nit**,
or a **silently-wrong default** reachable only in narrow content shapes — not a boundary
violation or a wrong value shipping broadly across all games.

| Dimension | CRITICAL | HIGH | MEDIUM | LOW | Findings |
|---|---:|---:|---:|---:|---|
| 1. Coordinate-system correctness | 0 | 0 | 1 | 4 | COORD-1..5 |
| 2. NIFAL mapping shape | 0 | 0 | 2 | 2 | NIFAL-D2-01..04 |
| 3. Material translation boundary | 0 | 0 | 2 | 2 | MAT-D3-01..04 |
| 4. PHYSAL | 0 | 0 | 2 | 0 | PHYS-01..02 |
| 5. EXAL | 0 | 0 | 2 | 6 | EXAL-01..08 |
| 6. Per-game translation patterns | 0 | 0 | 1 | 0 | PAT-D6-01 |
| 7. Subsystem coverage vs legacy | 0 | 0 | 4 | 1 | SUBSYS-01..05 |

---

## Remediated Earlier Today

Carried forward from the narrow PHYSAL closeout that previously occupied this report path
(base `7a851ab9`). Re-verified as still holding by this sweep's Dimension 4 (PHYSAL) pass —
`CollisionAuthoringSummary`/`needs_packed_havok_fallback` confirmed wired into
`cell_loader/spawn.rs`, `bhkSPCollisionObject` confirmed dispatching through
`BhkPCollisionObject`, `summarize_collision_authoring` confirmed to have real call sites,
and all four `extract_ragdoll` rejection paths confirmed to `log::warn!` with context. No
regression.

| ID | Severity | Previous failure | Closeout |
|---|---:|---|---|
| #2355 | MEDIUM | `BhkNPCollisionObject` fallback covered only `RenderLayer::Architecture`; packed-Havok clutter/actors could remain non-colliding. | `CollisionAuthoringSummary` now survives the parse/import cache boundary and drives a layer-aware spawn policy. Architecture retains precise static trimeshes. Packed Clutter/Actor content receives one conservative keyframed AABB proxy, parented to the visual placement and excluded from rendering. |
| #2332 | LOW | FO3 DLC `bhkSPCollisionObject` dispatched as `BhkCollisionObject`, erasing phantom semantics. | Dispatch now uses the byte-identical `BhkPCollisionObject` wrapper. A byte-exact regression test pins the 10-byte layout and phantom downcast. |
| #2333 | LOW | `CollisionAuthoring` was diagnostic-only; fallback behavior did not consume the classification. | `summarize_collision_authoring` reuses the classifier, cache entries retain the summary on synchronous and streaming imports, and the runtime fallback policy consumes it. |
| #2339 | LOW | Four ragdoll rejection paths were silent: unhosted bodies, unresolved shapes, non-finite body data, and unresolved constraint endpoints. | Each path now emits an actionable warning with block/bone/ref context. |

**Note (Issue #2355 numbering):** This same issue number, #2355, is also referenced in
Dimension 2 (NIFAL) below as the label for the *Starfield* collision-slice-unreachable gap
("SF-D8-04"). Both are legitimately the same open issue — the collision fallback policy
fixed above (packed-Havok proxy spawn) and the Starfield-specific "collision slice never
fires at all" gap are two write-ups of adjacent but distinct symptoms tracked under one
issue. Not a numbering error in this report; flagged here so a reader doesn't assume a typo.

---

## Dimension 1: Coordinate-system correctness (Z-up → Y-up)

**Invariants re-confirmed, no finding:** single `(x, z, -y)` source of truth in
`crates/core/src/math/coord.rs` (151 call sites, no new #1044-class duplication); strip
de-stitch still swaps the last-two vertices (`crates/nif/src/blocks/strip.rs::destrip`,
centralized under #2298); winding chain (`Camera::projection_matrix` Y-flip) unchanged; all
production `4096.0` exterior-grid consumers route through `EXTERIOR_CELL_UNITS` /
`cell_grid_to_world_yup`; `crates/nif/src/anim/coord.rs` is a pure re-export of the core
position/quaternion helpers with no parallel path; the four `C·R·Cᵀ` basis-change copies
(canonical Shepperd path, skin bind-inverse, Havok 4×4 decomposition, Havok quaternion
swizzle) were hand-verified numerically consistent.

### COORD-1: KF XYZ-Euler rotation keys use the CCW convention, contradicting Gamebryo's CW-positive rule every other Euler consumer honours
- **Severity**: MEDIUM
- **Dimension**: Coordinate-system correctness
- **Location**: `crates/nif/src/anim/keys.rs:102-132` (call), `:187-199` (`euler_to_quat_wxyz`)
- **Status**: NEW
- **Description**: `convert_xyz_euler_keys` handles `NiTransformData`/`NiKeyframeData` rotation keys stored as `Rotation Type == 4` (XYZ_ROTATION_KEY). It samples the three axes and composes with `euler_to_quat_wxyz(x, y, z)`, which builds standard CCW-positive elementary quaternions (`qx ⊗ qy ⊗ qz`, hand-expanded against the code). Gamebryo is clockwise-positive per the vendor header (`/mnt/data/src/reference/gamebryo-v32/Include/efd/Matrix3.h:18-36`: "positive angles are associated with clockwise rotations"), so a Gamebryo Euler triple must be negated before composition — every other Euler consumer in the tree (`euler_zup_to_quat_yup`, the REFR dispatcher, XCLL lighting) does negate. Conjugating the code's product through the Z-up→Y-up swap gives `Rx(rx)·Rz(-ry)·Ry(rz)` — character-for-character `--rotation-mode 3`, which `byroredux/src/cell_loader/euler.rs:79` itself labels a non-shipping diagnostic.
- **Evidence**: Reachability confirmed live (`crates/nif/src/anim/keys.rs:60-61` dispatches on `KeyType::XyzRotation`, fed from `crates/nif/src/blocks/interpolator.rs:308-330`). Existing tests (`crates/nif/src/anim/tests/coord_keys.rs:37-80`) only assert unit length and axis dominance — sign-blind, pass under either convention.
- **Impact**: Any animated node whose rotation is authored as XYZ Euler key groups rotates in the wrong direction (exact inverse for single-axis channels, a different rotation entirely for multi-axis ones) — limbs/doors/machinery counter-rotate or skew. Bethesda KF overwhelmingly ships quaternion keys, so scope is narrow but nonzero, and the failure is silent.
- **Suggested Fix**: Negate the three samples before composition (build the Z-up quat from `(-x,-y,-z)`), or add a `euler_zup_to_quat_yup_xyz` sibling to the core SoT rather than a second private formula in the animation crate. Add a sign-discriminating regression pin before shipping the flip; validate against a real asset that exercises `Rotation Type == 4`.

### COORD-2: Door/XTEL transition rotation bypasses the `--rotation-mode` dispatcher while its doc comment claims it uses it
- **Severity**: LOW
- **Dimension**: Coordinate-system correctness
- **Location**: `byroredux/src/cell_loader/transition.rs:160-166`
- **Status**: NEW
- **Description**: `rotation_zup_to_yup_quat` is documented as "wrapper over `euler_zup_to_quat_yup_refr` — same convention REFR placements use", but the body calls the plain canonical `euler_zup_to_quat_yup`, not the A/B dispatcher. It is the one caller converting a REFR-sourced Euler triple (XTEL teleport-destination rotation) that skips the dispatcher; all other REFR-family sites (`references/mod.rs:392`, `refr.rs:498`, `placement_lod.rs:171`) do use it.
- **Evidence**: Body vs. doc-link mismatch confirmed by direct read.
- **Impact**: Zero at the shipping default (mode 1 ≡ canonical). Under `--rotation-mode 0/2/3` the player lands at a door with an orientation from a different convention than the surrounding geometry — exactly the scenario the diagnostic flag exists to triage.
- **Suggested Fix**: Call `euler_zup_to_quat_yup_refr` to match the doc and the rest of the REFR family, or rewrite the comment to state the deliberate pin.

### COORD-3: `RENDER_ORIGIN_SNAP` is a second, uncoupled `4096.0` exterior-cell literal
- **Severity**: LOW
- **Dimension**: Coordinate-system correctness
- **Location**: `crates/renderer/src/vulkan/scene_buffer/constants.rs:339-352` (const), `:404-412` (test)
- **Status**: NEW
- **Description**: `RENDER_ORIGIN_SNAP: f32 = 4096.0` is a bare literal whose own doc comment names it as the exterior cell edge length, but its pin test asserts `== 4096.0` rather than `== EXTERIOR_CELL_UNITS`, even though `byroredux-renderer` already depends on `byroredux-core`. Residue of #1112/TD3-202's literal collapse — six sites unified, this seventh (added later, #1494) reintroduced the pattern.
- **Impact**: Latent — the value is spec-fixed today so the two constants cannot realistically disagree. Risk: the render-origin rebase and the cell-streaming grid must snap on the same boundary for the #1489 `prev_view_proj` origin correction to stay valid; an isolated retune of either breaks motion vectors across grid crossings with no test failure.
- **Suggested Fix**: `RENDER_ORIGIN_SNAP = byroredux_core::math::coord::EXTERIOR_CELL_UNITS` and update the pin test to assert against the SoT constant.

### COORD-4: Four independent copies of the `C·R·Cᵀ` basis change, coupled only by comments
- **Severity**: LOW
- **Dimension**: Coordinate-system correctness
- **Location**: `crates/nif/src/import/coord.rs:41-45`; `crates/nif/src/import/mesh/skin.rs:479-492`; `crates/nif/src/import/collision/mod.rs:518-522` and `:487-494` (quaternion flavour)
- **Status**: NEW (structural/regression-risk only — all four currently correct, hand-verified numerically)
- **Description**: The array-form position swap has one home; the rotation flavour does not. #1617 routed the translation halves of the Havok/skin sites into the SoT but deliberately left the matrix/quat math duplicated. This is the precise shape of the pre-#1044 bug class (five copies, one missing the #333 normalise fix, drifted for months).
- **Impact**: None currently. A future fix (handedness/determinant guard) applied to one copy will not propagate to the other three, with no test to catch the divergence.
- **Suggested Fix**: Promote `zup_to_yup_rot_mat3` into `crates/core/src/math/coord.rs`; at minimum add a cross-checking unit test that feeds one random rotation through all four paths and asserts agreement.

### COORD-5: `cell_rot_sweep` example hand-copies the four-mode Euler dispatcher
- **Severity**: LOW
- **Dimension**: Coordinate-system correctness
- **Location**: `crates/plugin/examples/cell_rot_sweep.rs:22-27`
- **Status**: NEW
- **Description**: The example reproduces all four rotation-mode formulas verbatim (byte-identical to `byroredux/src/cell_loader/euler.rs:67,73,76,79` today) instead of calling the dispatcher.
- **Impact**: Example-only, no shipping-path risk — but the example exists specifically to triage REFR rotation disputes; if the dispatcher is retuned and the example isn't, the sweep reports conclusions about a convention the engine no longer uses.
- **Suggested Fix**: Move the four-mode match into a shared `pub` function both the dispatcher and the example call.

**Dedup:** keyword-scanned `issues.txt` for `coordinate|z-up|euler|winding|rotation-mode|4096|axis|quat`; only hit was #2302 (unrelated ref-validation issue). All five findings NEW; no regression of #1044/#1112/#333/#2298/#1617 (each re-verified holding).

---

## Dimension 2: NIFAL — canonical NIF→ECS translation contract (mapping shape)

**Per-category boundary status** (verified single-producer unless noted):

| Category | Boundary | Single producer? | Notes |
|---|---|---|---|
| material | `material_translate.rs::translate_material` | Y | Second write-site tracked as Existing #2330 |
| geometry/transform | `mesh/{ni_tri_shape,bs_tri_shape,bs_geometry}.rs` + `coord.rs` | Y | Raw-tier producers (spt, precombine) feed the same shape, permitted |
| skinning | `mesh/skin.rs` → `nif_loader.rs:1001` `new_with_global` | Y | Two shape gaps: NIFAL-D2-02, NIFAL-D2-03 |
| **lights** | NIF half only (`spawn.rs:779`); **no ESM boundary function** | **N** | → NIFAL-D2-01 |
| nodes | No single boundary by design (documented triaged) | N/A | Re-verified; 4 raw-parked fields still zero consumers |
| particles | `systems/particle.rs:64` `apply_emitter_overlays` | Y | Starfield gap Existing #2354 |
| collision | `import/collision/shape.rs:42` `resolve_shape` | Y (authored) | Synthesized fallbacks data-driven, not game-keyed; Starfield gap Existing #2355 |
| animation | `anim_convert.rs::convert_nif_clip` + `asset_provider/animation.rs::convert_hkx_clip` | Y (both declared) | Naming-shape nit → NIFAL-D2-04 |
| shader flags/texture sets | `import/material/mod.rs:1227` | Y | Renderer/core/physics crates: zero `GameKind`/`bsver` occurrences confirmed |

**Downstream per-game-branch scan:** `grep GameKind::|NifVariant|bsver` over `crates/renderer`, `crates/core`, `crates/physics` → 0 hits. Every `byroredux/src` hit sits inside a translate/asset-resolution boundary (light-flag translate, env_translate, LOD-format pre-parse selection, asset-path selection) — never on already-canonical data. No boundary violation found.

### NIFAL-D2-01: ESM-sourced lights never reach `LightKind` — three of four canonical `LightSource` producers hard-default to `Point`
- **Severity**: MEDIUM
- **Dimension**: NIFAL mapping shape
- **Location**: `byroredux/src/cell_loader/spawn.rs:1734`, `byroredux/src/cell_loader/references/mod.rs:1224,1310` (producers); `crates/plugin/src/esm/cell/support.rs:85-133` + `crates/plugin/src/esm/cell/mod.rs:565-600` (dropped authored signal)
- **Status**: NEW
- **Description**: nifal.md §2 marks "Lights — converged", true only for the NIF half. There is no `translate_light` boundary: the canonical `LightSource` is constructed at four independent sites, and only the direct-`NiPointLight` path populates `kind`/`direction`/`outer_angle`. The three ESM-LIGH-sourced producers hand-copy scalar fields and take `..Default::default()` → `LightKind::Point`. The authored spot signal is reachable — `LIGHT_FLAG_SHADOW_SPOTLIGHT` (0x400) survives into `LightSource.flags`, and the LIGH `DATA` cone angle at bytes 20-23 is named in the parser's own layout comment ("FOV (spot light)") but never read into `LightData`. Same shape as the already-fixed #2205, one tier up.
- **Evidence**: `render/lights.rs:200-231` correctly consumes `light.kind` (Spot → cone math) — the consumer is ready and unused.
- **Impact**: Every ESM-placed spotlight in every supported game renders as a full omnidirectional point light over its authored radius — cone-directed lanterns, searchlights, FO4/Skyrim directed fixtures spill light backwards through their own housings.
- **Suggested Fix**: Read LIGH `DATA` bytes 20-23 into `LightData.fov_degrees` (and Starfield's `DAT2` equivalent); introduce a `translate_light(ld, game) -> LightSource` boundary beside `canonical_light_shadow_flags` deriving `kind` from the spot flag + FOV; collapse all four producers onto it.

### NIFAL-D2-02: `ImportedMesh.skin` is consumed on the loose-NIF path only — cell-loaded skinned geometry never gets a canonical `SkinnedMesh`
- **Severity**: MEDIUM
- **Dimension**: NIFAL mapping shape
- **Location**: `byroredux/src/cell_loader/spawn.rs:1681` (only cell-path use, as a boolean filter); `byroredux/src/scene/nif_loader.rs:955-1001` (sole `SkinnedMesh` producer)
- **Status**: NEW
- **Description**: `ImportedSkin` is populated identically on both load paths by the shared mesh extractors, but only the loose-NIF path translates it into canonical `SkinnedMesh` (`new_with_global`, one production caller). On the cell-loader path the field is read exactly once — as a negative filter for the architecture-trimesh collider fallback — and never turned into a bone binding. `grep SkinnedMesh` under `cell_loader/` → 0 hits. Structurally identical to the acknowledged #2206 class (billboard_mode: correct on loose path, silently absent on cell path), but skinning is not listed in nifal.md §2's passthrough parity table — it's marked flatly "converged".
- **Impact**: Any cell-placed REFR with skinned geometry (Skyrim/FO4 wind-animated cloth banners, chains, hanging/moveable statics using `NiSkinInstance`) spawns with skin data parsed and per-vertex weights uploaded, but no palette binding — renders frozen in bind pose, never animates. Silent: no `log::warn`, because the translation step doesn't exist at all. NPC actors unaffected (they route through the loose path).
- **Suggested Fix**: Extend the cell spawn path to build `SkinnedMesh` from `mesh.skin` against the placement's own node map, or — if measurement shows negligible content — record the gap explicitly in nifal.md §2's passthrough table rather than leaving the category marked bare "converged".

### NIFAL-D2-03: `SkinnedMesh.bones: Vec<Option<EntityId>>` is an unresolved-reference sentinel nifal.md's "skinning — converged" entry doesn't record
- **Severity**: LOW
- **Dimension**: NIFAL mapping shape
- **Location**: `crates/core/src/ecs/components/skinned_mesh.rs:63-69,184-201`; producer `byroredux/src/scene/nif_loader.rs:966-1024`
- **Status**: NEW
- **Description**: `bones`/`skeleton_root` carry `Option`s past the boundary; `compute_palette_into` substitutes identity for `None`. Thoroughly documented and logged at the component/producer level (not a silent leak) — but nifal.md §2 marks skinning "converged" with no residual note.
- **Impact**: Documentation-shape only. The concrete risk is a future audit reading "converged" and skipping the check — the exact #2206 failure mode.
- **Suggested Fix**: Add a one-line residual note to nifal.md §2 recording the `Option` as a terminal "bone-name lookup failed" state, not a resolve-later leak.

### NIFAL-D2-04: A raw-tier `AnimationClip` shares its name with the canonical one while nifal.md asserts "no parallel struct"
- **Severity**: LOW
- **Dimension**: NIFAL mapping shape
- **Location**: `crates/nif/src/anim/types.rs:183` (raw) vs `crates/core/src/animation/types.rs:186` (canonical); claim at `docs/engine/nifal.md:242-244`
- **Status**: NEW
- **Description**: nifal.md states "no parallel struct" for `AnimationClip`; `byroredux_nif::anim::AnimationClip` (raw tier, tier-model-permitted) does exist and is correctly type-qualified at all call sites — the defect is only that the doc's phrasing denies it, making a grep-based single-producer check ambiguous.
- **Impact**: None at runtime; costs audit precision.
- **Suggested Fix**: Reword nifal.md, or rename the raw type to `ImportedAnimationClip` matching the rest of the `Imported*` convention.

**Verified-clean, not filed:** `translate_material` single-boundary intact; `apply_emitter_overlays` covers all five authored override classes at both sites; `resolve_shape` is the single authored-collision translator with data-driven (not game-keyed) fallbacks; the four raw-parked `ImportedNode` fields confirmed still consumer-less.

---

## Dimension 3: Material translation boundary (NIFAL reference slice)

**Verification summary (clean):** `translate_material` confirmed the sole production `Material` producer (only other struct literals are the self-contained `--cornell` RT harness and `#[cfg(test)]` sites); both spawn paths delegate; no per-game branch inside `translate_material`; `resolve_pbr()` runs before `classify_glass_into_material`, so forced glass roughness correctly wins (pinned by 11 tests); no render-time re-classification survives in `render/static_meshes.rs` beyond explanatory comments; only two `Material` write sites exist workspace-wide (`material_translate.rs:368` = Existing #2330, and the `mat.set` debug console); recent git history on both files is clean (no reintroduced `Option`-override path); the emissive 3-variant scale and `NiFogProperty` deliberate skip regression guards both hold.

### MAT-D3-01: `grayscale_to_palette_scale` dead-ends at the NIFAL boundary — captured by both importers, no canonical `Material` field to land in
- **Severity**: MEDIUM
- **Dimension**: Material translation boundary
- **Location**: `byroredux/src/material_translate.rs:120-215` (no copy); `crates/core/src/ecs/components/material.rs:55-304` (no field); producers `crates/nif/src/import/types.rs:489`, `byroredux/src/asset_provider/material.rs:1065-1067`, `crates/nif/src/blocks/shader.rs:724`
- **Status**: NEW (same shape as fixed #2284, distinct field)
- **Description**: `ImportedMaterial.grayscale_to_palette_scale` is populated from both the inline NIF shader block (BSVER≥130 `BSLightingShaderProperty`/`BSEffectShaderProperty`) and the BGSM/BGEM merge (with parent-template precedence + a dedicated round-trip test), but `translate_material` never copies it — the canonical `Material` has no such field. `triangle.frag:984-987` documents this explicitly: "not yet plumbed to GpuMaterial — direct lookup for now." #2284's landing comment names this exact field as the precedent that justified its own fix — this is the one remaining instance of that pattern.
- **Impact**: FO4/FO76/Starfield content authoring a sub-1.0 palette scale (de-saturating a shared greyscale ramp) renders the palette remap at full strength; because `EFFECT_PALETTE_COLOR`/`ALPHA` is a replace not a blend, an authored 0.5 scale that should soften the remap produces the full palette colour instead.
- **Suggested Fix**: Add `grayscale_to_palette_scale: f32` (default 1.0) to `Material` and copy it in `translate_material` (closes the silent drop); plumb to `GpuMaterial`/shader as a separate follow-up.

### MAT-D3-02: Three exterior draw populations never reach `translate_material` — LAND terrain, terrain LOD, and object LOD carry no `Material` at all
- **Severity**: MEDIUM
- **Dimension**: Material translation boundary
- **Location**: `byroredux/src/cell_loader/terrain.rs:589-624`; `terrain_lod.rs:672-684`; `object_lod.rs:319-333`; consumed at `render/static_meshes.rs:323-338`
- **Status**: NEW (adjacent to open #2371, which doesn't mention the missing `Material`)
- **Description**: All three spawners insert `Transform`/`GlobalTransform`/`MeshHandle`/`TextureHandle`/`RenderLayer` but never a `Material`, so their draws fall into `static_meshes.rs`'s `else` arm and get an 11-tuple of hardcoded literals (`roughness 0.5`, `metalness 0.0`, etc.) — a second materialization site living in the render path, outside the documented single source of truth.
- **Impact**: (a) Exterior landscape shades with a markedly tighter/brighter GGX lobe than the stone/dirt statics on it (0.5 vs the classifier's 0.85), a visible mismatch at every ground-meets-architecture seam. (b) Object LOD imposters carry roughness 0.5 while the full models they swap to carry the classifier value — a shading pop on top of the geometric LOD pop. (c) The NIFAL invariant "every drawn surface's canonical material is produced at one boundary" is false for the entire outdoors.
- **Suggested Fix**: Give the three spawners a canonical `Material` — for LAND, feed the resolved base-layer texture path through `Material{..}` + `resolve_pbr()` (reuses the existing classifier); for object LOD, carry the source record's material through the imposter. If a flat default is deliberately preferred for LOD, insert an explicit `Material::default()` component so it's owned and visible to `mat.*` tooling.

### MAT-D3-03: The "cannot diverge" claim on the normal-alpha-as-spec pair is false for `Material`-less entities
- **Severity**: LOW
- **Dimension**: Material translation boundary
- **Location**: `render/static_meshes.rs:405-427`; `material_translate.rs:249-268,334-347`
- **Status**: NEW
- **Description**: A comment asserts the per-draw gloss-slot binding and the spawn-side scalar resolve "cannot diverge" because they share a predicate — true, but the spawn write-back silently no-ops on any entity without a `Material` (early-return), while the render-side gate has no such guard and substitutes fallback values that still pass the gate.
- **Impact**: Behaviourally small (still darkens rather than brightens), but the asserted invariant isn't enforced — exactly the trap a future `Material`-less population (MAT-D3-02) would fall into.
- **Suggested Fix**: Fix MAT-D3-02 (removes all `Material`-less draws), or gate the render-side binding on `mat.is_some()`.

### MAT-D3-04: Four comments describe the decal-layer escalation with the wrong input field and the wrong output layer
- **Severity**: LOW
- **Dimension**: Material translation boundary
- **Location**: `cell_loader/spawn.rs:405,1585-1591`; `scene/nif_loader.rs:848-852`; `render/static_meshes.rs:119`
- **Status**: NEW
- **Description**: Comments describe escalation via `alpha_test_func != 0` → `RenderLayer::Decal`; actual code passes the `alpha_test` bool (not `alpha_test_func`, which is never read at these sites) and escalates to `RenderLayer::Clutter`, not `Decal` (only `is_decal` yields Decal, per the helper's own doc comment).
- **Impact**: Documentation only, no runtime effect — but misdescribes a depth-bias rule at four sites including the render hot path.
- **Suggested Fix**: Correct all four comments to match the actual `is_decal`/Clutter behavior.

**Dedup:** not re-filed: #2330 (second roughness write site), #2359 (Starfield .mat merge), #2296/#2297 (material_kind cross-crate pinning), emissive 3-variant scale, `NiFogProperty` skip.

---

## Dimension 4: PHYSAL — per-game Havok articulation → solver

**Boundary contract re-checked and intact:** the constraint CInfo decode remains the only per-game seam; `extract_ragdoll` switches on `BhkConstraintData` only, never on game; the single-translate/single-build chain (`ragdoll.rs::template_from_imported`/`activate_ragdoll` → `physics/src/ragdoll.rs::build_ragdoll`) carries no Rapier types upstream of the solver boundary; writeback (`ragdoll_writeback_system`) only touches `GlobalTransform`. All four items from the earlier-today closeout (#2355/#2332/#2333/#2339) re-verified holding, no regression.

### PHYS-01: `extract_ragdoll` applies `BhkRigidBody` CInfo translation/rotation unconditionally, bypassing the `is_t` gate its sibling extractor requires
- **Severity**: MEDIUM
- **Dimension**: PHYSAL (source boundary/extract)
- **Location**: `crates/nif/src/import/collision/ragdoll.rs:90-104`, contrast `crates/nif/src/import/collision/mod.rs:316-334` (`extract_from_classic`)
- **Status**: NEW
- **Description**: `extract_from_classic` gates applying a `BhkRigidBody`'s CInfo translation/rotation on `body.is_t` — only `bhkRigidBodyT` activates the offset; plain `bhkRigidBody` carries the same wire fields but Gamebryo treats them as identity even when stale/non-zero bytes survive in vanilla content (fixed under #2316 specifically because applying non-T bytes displaced FO3 architecture colliders). `extract_ragdoll`, building ragdoll bodies from the same block type, reads `body.translation`/`body.rotation` unconditionally with no `is_t` check.
- **Impact**: If a ragdoll bone is authored as plain `bhkRigidBody` carrying stale non-identity translation/rotation bytes — the exact pattern #2316 fixed for architecture — the extractor applies that garbage offset to the body's rest-space pose, propagating through `template_from_imported`'s rest-pose delta into every activation's world-space seed, displacing/misrotating the ragdoll body relative to its bone. Unconfirmed on vanilla content either way (no test pins ragdoll bones as always-T); no comment explains the asymmetry with the sibling extractor.
- **Related**: #2316 (the sibling fix this extractor doesn't mirror)
- **Suggested Fix**: Mirror `extract_from_classic`'s `is_t` gate in `extract_ragdoll`, or add a comment citing real-corpus evidence that ragdoll bones are always `bhkRigidBodyT` if that's actually true.

### PHYS-02: LimitedHinge's authored perp-axis zero-reference is parsed then discarded — every elbow/knee joint's angle limits apply around a synthesized, not authored, reference frame
- **Severity**: MEDIUM
- **Dimension**: PHYSAL (extract→translate boundary)
- **Location**: `crates/nif/src/blocks/collision/constraints.rs:150-218` (decoded, byte-pinned); `crates/nif/src/import/collision/ragdoll.rs:324-333` (`limited_hinge_joint` — perp fields never read); `crates/nif/src/import/types.rs:1026-1033` (no perp field on `ImportedJointKind::LimitedHinge`); `crates/physics/src/ragdoll.rs:381-386` (`build_joint` synthesizes an arbitrary perp via `any_perp(axis)`)
- **Status**: NEW
- **Description**: `bhkLimitedHingeConstraintCInfo` authors "perp axis" vectors defining the hinge's zero-angle reference frame — the plane `min_angle`/`max_angle` are measured from. `LimitedHingeCInfo` decodes and byte-pins all four vectors, but the extract→canonical step reads only `axis_a`/`pivot_a`/`axis_b`/`pivot_b`; the perp vectors are read into the struct and never touched again — `ImportedJointKind::LimitedHinge` has no field for them. At the solver boundary, `build_joint` explicitly synthesizes an arbitrary orthogonal frame instead, with a comment acknowledging "only the limit's zero-reference is offset."
- **Impact**: The real-data reference test confirms FNV elbows/knees decode as `LimitedHinge`, and Oblivion/Skyrim baselines list 7/8 `bhkLimitedHingeConstraint` blocks per skeleton — this is every elbow and knee joint on every converged game (Oblivion/FO3/FNV/Skyrim), not an edge case. The enforced angle window is applied relative to an arbitrary synthesized zero-reference rather than the authored one, so the actual swing range is rotated by an uncontrolled per-joint amount from what the content author intended — visible implausible bending (locking straight, bending backward, clamping short) once a ragdoll activates. Not listed in physal.md §3/§5's documented-approximation list — currently an unacknowledged fidelity loss, distinct from the already-documented Ragdoll-type `plane_min`/`plane_max` simplification (#1982).
- **Related**: #1982 (the analogous, already-documented Ragdoll-type simplification this is the LimitedHinge sibling of)
- **Suggested Fix**: Thread `perp_axis_in_a1`/`b1` through `ImportedJointKind::LimitedHinge` and use as the authored secondary axis in `frame_rot` at the solver boundary, matching how the Ragdoll type already threads `plane_a`/`plane_b`. At minimum, add the same explicit doc-comment acknowledgment physal.md gives the Ragdoll-type approximation.

**Dedup:** checked against issues.txt, neither present. Documented limitations re-confirmed, not re-filed: FO4+/FO76/Starfield packed-body fidelity (blocked on `BhkSystemBinary`); Havok cone+2-plane approximation; motors captured-but-unused.

---

## Dimension 5: EXAL — per-game exterior environment → renderer

**Boundary verification (no findings):** `env_translate.rs` confirmed the sole producer of `SkyParamsRes`/`WeatherDataRes` (all other literals are `#[cfg(test)]`); all three exterior entry paths (bulk `--grid`, streaming bootstrap, debug exterior load) call `apply_worldspace_weather`; `--cornell` reuses the procedural-fallback constructors, not a second producer; water is single-site (`default_water_for_worldspace`, `resolve_water_material` each have exactly one production caller); no render-loop environment fallback (`SUN_INTENSITY_PEAK` unwrap_or chased and disproved as reachable). False premises explicitly not filed: per-worldspace latitude parsing, "object LOD unimplemented", "Oblivion `_far.nif` scheme unimplemented", "`.btr` terrain LOD unimplemented".

### EXAL-01: WRLD `NAM3`/`NAM4` LOD-water is parsed but no LOD-ring water plane is ever spawned
- **Severity**: MEDIUM
- **Dimension**: EXAL
- **Location**: `crates/plugin/src/esm/cell/wrld.rs:151-169`; `crates/plugin/src/esm/cell/mod.rs:844-861`; `byroredux/src/cell_loader/water.rs:77`; `terrain_lod.rs`, `object_lod.rs`
- **Status**: NEW (concrete sub-finding under open epic #2373)
- **Description**: `WorldspaceRecord::lod_water_form`/`lod_water_height` (landed #1849) are read by nothing. `spawn_water_plane` has exactly two production call sites (full-detail exterior cell, interior cell) — none of the four LOD providers emits a water surface. The parser's own doc comment records NAM3≠NAM2 on 18/28 Fallout3.esm worldspaces and NAM4≠DNAM on 22/30 Skyrim.esm worldspaces, so full-detail values cannot substitute.
- **Impact**: Open-world oceans/lakes terminate at the streaming ring boundary on every game — the classic "dry ocean" artifact beyond `radius_unload`. A naive fix reusing NAM2/DNAM would place the LOD sheet at the wrong Z on the majority of worldspaces.
- **Suggested Fix**: Add a `translate_lod_water(wrld)` arm to `env_translate.rs` reading NAM3/NAM4 (with the documented Oblivion `None` sentinel), spawn one large `IsLodTerrain`-marked water quad per ring clipped to exclude the full-detail radius.

### EXAL-02: Worldspace climate resolution ignores the WRLD parent chain (`WNAM`/`PNAM`) — child worldspaces silently get the procedural Mojave sky
- **Severity**: MEDIUM
- **Dimension**: EXAL
- **Location**: `byroredux/src/cell_loader/exterior.rs:799-817`; `crates/plugin/src/esm/cell/wrld.rs:215-217`; `crates/plugin/src/esm/cell/mod.rs:821-826`
- **Status**: NEW (concrete sub-finding under #2373/#2369)
- **Description**: Climate resolution is a single flat lookup populated only when a WRLD authors its own `CNAM`. `parent_worldspace`(WNAM)/`parent_flags`(PNAM) are parsed with zero consumers repo-wide. A child worldspace inheriting climate from its parent resolves to `None` → `apply_worldspace_weather` installs the procedural-fallback Mojave desert sky.
- **Impact**: Any child worldspace relying on PNAM inheritance (Skyrim's DLC/holdout worlds, FO4 sub-worlds, Oblivion-plane worlds) renders the wrong sky/fog/sun. Silent — the fallback is an intentional canonical default, so nothing logs the inheritance miss; presents as a weather bug, not an inheritance gap.
- **Suggested Fix**: Chase `parent_worldspace` when no own CNAM exists, gated on the PNAM inherit bit, inside `env_translate.rs`; log at `warn` when the chain terminates unresolved.

### EXAL-03: CELL `XCCM` per-cell climate override is parsed with zero consumers
- **Severity**: LOW · **Status**: NEW (sub-finding under #2373)
- **Location**: `crates/plugin/src/esm/cell/wrld.rs:385`, `walkers.rs:318`, `mod.rs:240`; `byroredux/src/scene/world_setup.rs:240`
- **Description**: `CellRecord::climate_override` (XCCM) parsed on both CELL walk paths, asserted in tests, never read. No per-cell weather re-resolve hook exists at all.
- **Suggested Fix**: Wire a per-cell weather re-resolve at the boundary, or document as a deliberate non-goal in exal.md §2.

### EXAL-04: The "prebaked combined-LOD games" predicate is duplicated inline in two providers instead of one named `GameKind` decision
- **Severity**: LOW · **Status**: NEW
- **Location**: `object_lod.rs:164-169`, `terrain_lod.rs:367` (inline `matches!`), vs `placement_lod.rs:306-308` (named `placement_lod_supported`)
- **Description**: exal.md §4 requires one `GameKind`-keyed decision per quirk; the `.bto`/`.btr` "baked combined LOD" quirk is written twice as an identical inline literal instead of a shared named predicate.
- **Suggested Fix**: Add `baked_lod_supported(game: GameKind)` next to `placement_lod_supported`, call from both sites, with the same per-variant unit test.

### EXAL-05: `climate_tod_hours` — a canonical environment default — lives outside the EXAL boundary module
- **Severity**: LOW · **Status**: NEW
- **Location**: `byroredux/src/scene/world_setup.rs:190-214`, consumed at `env_translate.rs:363`
- **Description**: The CLMT `TNAM` decode + its hardcoded no-data fallback and corruption guard live in `world_setup.rs`, which `env_translate::translate_weather` reaches back out to — a single implementation (not a duplicate-producer finding) but living outside the module exal.md §3 designates as home, inverting the intended dependency direction.
- **Suggested Fix**: Move `climate_tod_hours`+`FALLBACK` into `env_translate.rs` verbatim; behavior-preserving, ~20 lines.

### EXAL-06: REGN is parsed too thinly to satisfy its own consumer plan — `RDAT` dropped at the parser tier
- **Severity**: LOW · **Status**: Existing: #2372 (scoping correction, not a duplicate)
- **Location**: `crates/plugin/src/esm/records/misc/world.rs:64-97`; `index.rs:85-86`; `cell/mod.rs:245-249`
- **Description**: #2372's acceptance criteria assume REGN drives ambient sound/fog/weather overlays/ground cover via RDAT sub-records; `RegnRecord` currently captures only `EDID`/`WNAM`/`RCLR` and its own doc comment states RDAT is explicitly out of scope. `index.regions`/`CellRecord::regions` (XCLR) also have zero consumers.
- **Suggested Fix**: Amend #2372's body to note the RDAT parse prerequisite so the estimate covers parse+translate+consume, not consume alone.

### EXAL-07: VWD consumer gap — confirmed still real, #2371 scopes it correctly
- **Severity**: LOW · **Status**: Existing: #2371 (confirmed, no action)
- **Location**: `esm/reader.rs:388`; `cell_loader/references/mod.rs:1558,1597-1604`; `components.rs:144-165`
- **Description**: `is_visible_when_distant()` flows into a `VisibleWhenDistant` marker component that no system queries — inert, exactly as exal.md §5.2 documents. Currently harmless: full REFRs spawn only inside `radius_unload`, both LOD rings load strictly outside it, so a full model and its LOD proxy provably never coexist.
- **Suggested Fix**: No action — keep under #2371; becomes load-bearing only if the full-detail radius is ever decoupled from the streaming ring.

### EXAL-08: WRLD `OFST` cell-offset table captured as raw words with no interpretation and no consumer
- **Severity**: LOW · **Status**: NEW (sub-finding under #2371)
- **Location**: `crates/plugin/src/esm/cell/wrld.rs:170-190`; `cell/mod.rs:862-874`
- **Description**: `cell_offsets` (OFST, #1849) is stored raw with the parser's own comment deferring interpretation "to a future LAND streamer" — that streamer now exists and doesn't use OFST (enumerates `index.exterior_cells` keys instead). Zero readers.
- **Impact**: Low — current approach works; cost is a per-worldspace `Vec<u32>` up to ~44k entries held for no benefit, and a parsed field that reads as a live capability when it isn't.
- **Suggested Fix**: Drop the capture with a note in exal.md §5 that OFST was superseded, or gate behind a feature flag until a consumer exists.

**Dedup:** EXAL-01/02/03/04/05/08 NEW; EXAL-06/07 are scope-corrections on existing #2372/#2371, not duplicates.

---

## Dimension 6: Per-game translation-survey gaps (Pattern A/B/C upstream branches)

**Verified NOT findings (survey premise now stale):** Pattern A's proposed `bsver()`→helper migration was attempted (#1277) and reverted after causing a regression (#982/#1838) — raw comparisons are now deliberately kept with named-constant thresholds and inline rationale comments; re-proposing the migration would re-file an already-reverted fix. Pattern B's `GameVariant` trait never landed but has no observed behavior divergence — not a finding. Pattern C's `CellLighting` flat struct has every `Option` field doc-commented per-game-era — matches the whitelisted "canonical sentinel" pattern. XCLL byte-length dispatch, SCOL/PKIN/MOVS/MSWP gating, `bhkNPCollisionObject` drop (documented PHYSAL limitation), and REFR `DATA` uniformity were all re-verified resolved or false-premise.

### PAT-D6-01: Skyrim+/FO4/FO76/Starfield RACE `DATA` sub-record is never decoded
- **Severity**: MEDIUM
- **Dimension**: Per-game translation-survey gaps
- **Location**: `crates/plugin/src/esm/records/actor/mod.rs:1024-1057` (`parse_race`)
- **Status**: NEW
- **Description**: `parse_race`'s `DATA` arm is gated `Oblivion | Fallout3NV` (fixed under #1629 to stop mis-decoding Skyrim's 128/164-byte layout with the 36-byte TES4/FO3/FNV field order), but no replacement arm was ever added for Skyrim LE/SE/FO4/FO76/Starfield — those fall through to `_ => {}`, so `skill_bonuses`/`base_height`/`base_weight`/`race_flags` stay at hardcoded defaults for every Skyrim+ RACE record. The gap is self-documented (line 1032 comment) but unresolved.
- **Evidence**: Confirmed zero production consumers anywhere in the tree for these fields on any game — currently dormant since nothing reads them yet, even on the correctly-parsed games.
- **Impact**: No visible behavior divergence today (dormant), but this will surface silently-wrong (uniform 1.0/1.0 height-weight, empty skill bonuses) the moment per-race scaling or skill-bonus application is wired up for Skyrim+ — with no error, no log, and no test coverage (existing RACE assertions are OBL-only).
- **Related**: Follow-up half of #1629 (which stopped the *wrong* decode; this is the deferred "add the *right* decode")
- **Suggested Fix**: Add a `Skyrim | Fallout4 | Fallout76 | Starfield` arm decoding the TES5+ 128/164-byte layout. At minimum add a `log::debug!` note when a non-OBL/FO3NV RACE `DATA` is skipped, mirroring the `xcll_size_sanity_warn` pattern.

**Dedup:** checked against issues.txt; not present. Six other survey-catalogued items re-verified resolved/false-premise, not re-filed.

---

## Dimension 7: Subsystem coverage vs legacy (fidelity gaps)

**Verification caveat:** the Gamebryo 2.3 source tree was not mounted in this sandbox; findings below are derived from Redux-side code plus `docs/legacy/api-deep-dive.md`. Where a claim depends on unread legacy runtime semantics, severity is reduced accordingly (SUBSYS-04).

**Disproved candidates (checked, not filed):** `WorldBound` is a real two-sphere enclosing-merge implementation with genuine two-pass propagation, not a stub. `Transform.scale`-collapses-non-uniform-scale is false as stated (NIF/Gamebryo `NiTransform` also carries one uniform scale float, no per-axis field to collapse) — the real loss is matrix-embedded, filed as SUBSYS-01. All 12 `NiProperty` types dispatch and 11 are wired (`NiDitherProperty` correctly ignored as a no-Vulkan-analogue hint; `NiFogProperty` is the documented #1224 skip). `MaterialInfo.stencil_state` dormancy is already tracked (#337). `StringPool` case-folding is a deliberate, correct match for `GlobalStringTable` — the defect is the *inconsistency* between regimes, filed as SUBSYS-03. Every animation channel type has a conversion arm — the renderer-end gap is filed as SUBSYS-05.

### SUBSYS-01: Scale/shear baked into `NiTransform.rotation` is silently discarded at parse time
- **Severity**: MEDIUM
- **Dimension**: Subsystem coverage vs legacy
- **Location**: `crates/nif/src/rotation.rs:11-64`; `crates/nif/src/import/coord.rs:33-63`; `crates/nif/src/stream.rs:679,702`
- **Status**: NEW
- **Description**: A non-orthonormal 3×3 (exporter-baked uniform scale, non-uniform scale, or shear) is destroyed rather than decomposed. `sanitize_rotation`, for `|det−1| ≥ 0.1`, replaces the matrix with the nearest orthogonal one via SVD, discarding the singular values instead of folding them into `NiTransform.scale`. Matrices inside the `det≈[0.9,1.1]` window but still non-orthonormal (e.g. `diag(2, 0.5, 1)`) take the fast path and are force-normalised by the #333 unit-quaternion guard. Nothing is logged either way.
- **Evidence**: `crates/nif/src/import/tests/transform.rs:250-270` pins the *loss* directly: a parent rotation of `2·I` composed with a child at (3,4,5) asserts a composed translation of **(3,4,5)**; Gamebryo's `NiTransform::operator*` computes `translate + scale·(rotate·child.translate)`, which for this input is **(6,8,10)** — the matrix IS applied in the source engine.
- **Impact**: Any subtree under an exporter-baked scaled node is placed at the wrong offset by exactly the discarded scale factor. Silent — presents downstream as "mesh part in the wrong place" with no breadcrumb, and affected content cannot be enumerated. Rare in Bethesda vanilla (dedicated scale float used instead) but reachable in 3rd-party/modded NIFs and older NetImmerse-era content.
- **Suggested Fix**: In `sanitize_rotation`, decompose (fold SVD singular values' geometric mean into the caller's `NiTransform.scale`) rather than orthogonalise-and-discard. Minimum viable step: emit a rate-limited `log::warn!` when a *scaled* (not zeroed) degenerate matrix is detected, to measure real corpus incidence before committing to the decomposition work.

### SUBSYS-02: `NiVertexColorProperty` is suppressed by `NiMaterialProperty` on the legacy property chain (re-introduces the #435/N06 bug class)
- **Severity**: MEDIUM
- **Dimension**: Subsystem coverage vs legacy
- **Location**: `crates/nif/src/import/material/legacy_properties.rs:717-748` (gate at `:742`), interacts with `:133-154`
- **Status**: NEW
- **Description**: `apply_vertex_color_property` writes `vertex_color_mode` only `if !info.has_material_data`. The intent (#1208) was Skyrim-specific (stop an inherited `NiVertexColorProperty` overriding a `BSLightingShaderProperty` default), but `has_material_data` is *also* set by the ordinary pre-Skyrim `NiMaterialProperty` arm. Since the property chain walks in file order, a `NiMaterialProperty` visited before a `NiVertexColorProperty` (the common Oblivion/FO3/FNV property order) latches the gate shut, dropping every later vertex-color property regardless of direct-vs-inherited status. This is the identical failure the codebase already diagnosed and fixed once for the sibling UV-transform flag (#435/N06) — the fix there was to split out a narrow `has_uv_transform` flag; that lesson wasn't applied here.
- **Evidence**: Trace for `[NiMaterialProperty, NiVertexColorProperty]`: iteration 1 sets `has_material_data=true`; iteration 2 reaches the gate false and skips the write. The regression suite covers only BSL+NVCP and no-shader-property+NVCP — never `NiMaterialProperty`+NVCP, the dominant legacy shape.
- **Impact**: Order-dependent, silent. A pre-Skyrim shape authoring `SOURCE_IGNORE` gets vertex colours applied anyway (over-darkened Oblivion/FO3 architecture/clutter carrying baked-AO vertex colour the property explicitly disabled). A shape authoring `SOURCE_EMISSIVE` alongside `NiMaterialProperty` loses emissive routing, falling back to albedo modulation — the class of bug #695 fixed at the shader end (torches/glowing signs going flat-lit).
- **Related**: #435/N06 (identical failure shape, previously fixed for UV transform)
- **Suggested Fix**: Mirror the #435 remedy — add a dedicated `vertex_color_mode_consumed` flag set only by the sites that genuinely author vertex-colour intent, and gate on that instead of `has_material_data`. Extend the precedence test suite with a `NiMaterialProperty`-before-NVCP case.

### SUBSYS-03: Bone-name → entity binding is case-sensitive on the skinning + ragdoll paths but case-insensitive everywhere else
- **Severity**: MEDIUM
- **Dimension**: Subsystem coverage vs legacy
- **Location**: `byroredux/src/scene/nif_loader.rs:412,449,966-968,994-998,1089`; `byroredux/src/ragdoll.rs:83-104`; contrast `crates/core/src/string/mod.rs:40-88`
- **Status**: NEW
- **Description**: `StringPool` ASCII-lowercases every intern explicitly to match Gamebryo's `GlobalStringTable` behavior, so `Name(FixedString)` comparisons (and animation channel binding) are case-insensitive. The skeleton binding path bypasses that pool entirely: `node_by_name: HashMap<Arc<str>, EntityId>` is keyed on the raw, case-preserved NIF node name, and both the skin-bone lookup and the ragdoll template lookup do exact-match `Arc<str>` comparisons — byte-exact, case-sensitive. Two different normalisation regimes for the same conceptual identifier, visible across six adjacent lines in `nif_loader.rs`.
- **Evidence**: `external_skeleton` and the body/head NIF's own bone names come from independently authored string tables (`npc_spawn/resumable.rs:540,562` loads skeleton.nif separately from body/head NIFs), so the cross-file exposure is real, not theoretical.
- **Impact**: A case-only divergence between an armour/body NIF's skin bone list and skeleton.nif's node names silently unresolves that bone — `compute_palette_into` substitutes identity, producing the "exploded limb" artefact for skinning, or drops the ragdoll body (potentially below the 2-body floor, disabling ragdoll entirely) with a `log::warn!`. Vanilla content is self-consistent, so this mostly bites 3rd-party/modded skeletons and outfits — precisely ByroRedux's target compatibility surface, and Bethesda's own tooling is case-insensitive so mods have no incentive to be byte-exact.
- **Suggested Fix**: Key `node_by_name`/`rest_pose_by_name`/`SkeletonMap` on `FixedString` through the same `StringPool` used for `Name`, so every bone-name comparison shares one normalisation. Cheaper interim: add a lowercased-key fallback lookup with a `log::warn!` when it's what resolves, measuring real-content incidence.

### SUBSYS-04: `NiAVObject` `DISABLE_SORTING` is captured into `SceneFlags` but never reaches the alpha draw sort
- **Severity**: LOW
- **Dimension**: Subsystem coverage vs legacy
- **Location**: `crates/core/src/ecs/components/scene_flags.rs:41-44,81-84`; `byroredux/src/render/mod.rs:309-360`
- **Status**: NEW
- **Description**: `SceneFlags::DISABLE_SORTING` (0x0040) and its accessor are attached at both import paths but have zero consumers — `draw_sort_key` has no sorting-disabled lane, so authored "keep children in file order" intent is overridden by the global back-to-front alpha sort.
- **Impact**: Transparent geometry authored with explicit draw-order (layered glass, multi-card foliage/hair, nested effect planes) gets depth-sorted instead of file-ordered. Scored LOW because the flag's legacy runtime semantics could not be verified against the (unmounted) Gamebryo source, and no concrete misrendering has been attributed to it yet.
- **Suggested Fix**: Verify the flag's meaning against Gamebryo's `NiAlphaAccumulator`/`NiAVObject` headers when reachable, and measure real-corpus incidence via a `byro-dbg` scan; if ~0, close as a documented intentional skip (`NiFogProperty`-style) so future audits don't re-file it.

### SUBSYS-05: Animated *material* channels reach ECS components but no renderer consumer (scope correction on #2221)
- **Severity**: MEDIUM
- **Dimension**: Subsystem coverage vs legacy
- **Location**: `byroredux/src/systems/animation.rs:16-17,190-320`; `byroredux/src/anim_convert.rs:104-202`
- **Status**: Existing: #2221 (scope correction, not a new issue)
- **Description**: #2221 ("non-transform animation channels have no production sink — visibility/alpha/UV/morph/flipbook are runtime-dead") has partially drifted since filing: `AnimatedVisibility` and `AnimatedUvTransform` now *do* have production sinks, and the five light channels are applied directly onto `LightSource`. The residual dead set is narrower and different from the title: `AnimatedAlpha`, `AnimatedDiffuseColor`, `AnimatedAmbientColor`, `AnimatedSpecularColor`, `AnimatedEmissiveColor`, `AnimatedShaderColor`, `AnimatedShaderFloat`, plus `AnimatedMorphWeights` (already on the documented PARKED list).
- **Evidence**: Each of the eight component names returns zero hits across `byroredux/src/render/` and `crates/renderer/src/` outside the writer/scheduler/save-exclusion/debug-registration sites — decoded, converted, sampled, and written every frame, then discarded.
- **Impact**: Fading/pulsing legacy content (`NiAlphaController` curtains/doors/ghost fades, `NiMaterialColorController` emissive pulses) animates in the ECS but renders static, plus wasted per-frame write traffic.
- **Suggested Fix**: Update #2221's title/body to the corrected residual set so the next audit doesn't re-chase the already-fixed visibility/UV half. Fold `AnimatedAlpha` and the four `Animated*Color` overrides into the `GpuMaterial` upload at the same site that already consumes `AnimatedUvTransform`.

**Dedup:** SUBSYS-01/02/03/04 NEW; SUBSYS-05 is a scope-correction on existing #2221.

---

## Verification

This audit is a read-only source review; no build/test commands were run as part of it
(the individual findings above cite specific existing tests where relevant, e.g. the
`compose_degenerate_scaled_rotation_uses_svd` pin for SUBSYS-01, the 11 glass-classification
tests for the MAT-D3 order-of-operations check). No files were modified, no GitHub issues
were created or mutated.

## Summary

- **Findings:** 29 total (27 NEW, 2 scope-corrections on existing issues #2221/#2372,
  1 confirmed-no-action on existing #2371)
- **By severity:** 0 CRITICAL, 0 HIGH, 14 MEDIUM, 15 LOW
- **Remediated earlier today (carried forward, re-verified holding):** 4 (#2355, #2332,
  #2333, #2339)
- **Boundary health:** All three canonical-translation layers (NIFAL/EXAL/PHYSAL)
  structurally intact — zero per-game branches found downstream of any translate boundary,
  zero second-producer sites for canonical types beyond the two already-tracked exceptions
  (#2330, and the newly-filed MAT-D3-02 exterior-terrain gap).
- **Highest-value fixes:** NIFAL-D2-01 (ESM spotlights render as point lights, all games),
  NIFAL-D2-02 (cell-placed skinned statics never animate), MAT-D3-02 (exterior terrain/LOD
  bypass the material boundary entirely), SUBSYS-02/SUBSYS-03 (both re-introduce previously
  -fixed bug *classes* — vertex-color suppression mirrors #435, bone-name case mismatch is a
  genuinely new compatibility-surface risk for modded content).

Suggested next step:
```
/audit-publish docs/audits/AUDIT_LEGACY_COMPAT_2026-08-07.md
```
