# #3637: mesh/texture archive lookup is first-wins, should be last-wins

**Source**: `docs/audits/AUDIT_FO4_2026-08-30.md` — Dimension 1
**Severity**: MEDIUM
**Location**: `byroredux/src/asset_provider/texture.rs` — `TextureProvider::extract_mesh`, `TextureProvider::extract`, `TextureProvider::has_texture`, `build_texture_provider`

## Description

Mesh and texture lookup iterates the archive chain and returns the **first** hit;
`build_texture_provider` appends archives in raw `--bsa` / `--textures-bsa` CLI order with no
load-order alignment, no last-wins rule, and no shadow diagnostic. Bethesda's own precedence
is the opposite: a later archive overrides an earlier one.

## Evidence

Current code (verified 2026-08-30) — both extractors are plain first-wins loops:

```rust
pub(crate) fn extract_mesh(&self, path: &str) -> Option<Vec<u8>> {
    let normalised = normalize_mesh_path(path);
    for archive in &self.mesh_archives {
        if let Ok(data) = archive.extract(normalised.as_ref()) { return Some(data); }
    }
    None
}
```

`extract` is the same shape; `has_texture` uses `.any()`, which is order-insensitive and
therefore inconsistent with what `extract` will actually return.

MEASURED against the installed FO4 `Data/` (297 entries, 187 `.ba2`, 7 masters):

- **1,681 `_oc.nif` paths exist in BOTH `Fallout4 - MeshesExtra.ba2` and a DLC
  `- Main.ba2`.** All 1,681 differ byte-for-byte, **and all 1,681 name a different
  `BSPackedGeomObject::filename_hash`** (base → `ddf19a67` / `Fallout4 - Geometry.csg`;
  DLC → the DLC's own blob). Whichever archive is listed first decides which *bake* renders —
  silently, because the #2369 hash routing makes both decode cleanly.
- Total DLC mesh entries shadowed when the base archives are listed first: **1,871**
  (1,761 precombines + 110 ordinary meshes) — DLCCoast 633, DLCNukaWorld 559, DLCRobot 427,
  DLCworkshop03 249, DLCworkshop01 3.
- Textures: 4 DLC entries, plus **`DLCUltraHighResolution`: 3,851 of 3,851 entries (100%)
  collide with `Fallout4 - Textures*.ba2`** — the HD pack is entirely inert unless the user
  happens to list it first.

## Impact

The natural invocation — archives in the same order as `--master`/`--esm`, which is how every
documented FO4 command line in the repo is written — is exactly the broken one. DLC
precombine re-bakes are silently replaced by their base-game namesakes, and the entire HD
texture pack never loads. Nothing logs.

## Suggested Fix

Reverse-iterate the chain (or insert at the front on open) so later archives win, and log a
shadow count per archive at open time so a collision is visible rather than silent. Bring
`has_texture` into agreement with whichever `extract` resolves.

## Related

#2369 (CSG hash routing — the reason both bakes decode cleanly and the shadow is invisible),
#1590 (owning-plugin path model).

## Completeness Checks
- [ ] **SIBLING**: `extract`, `extract_mesh`, `has_texture` and any other chain walker (material `.ba2`, script `.pex` lookup in `asset_provider/script.rs`) must all adopt the same precedence
- [ ] **TESTS**: a regression test pins a path present in two archives resolving to the later-listed one, plus the shadow-count diagnostic firing


---

# #3639: smoothness==1.0 with no gloss map pins roughness at 0.04 floor

**Source**: `docs/audits/AUDIT_FO4_2026-08-30.md` — Dimension 2
**Severity**: LOW
**Location**: `byroredux/src/asset_provider/material.rs` (`roughness_override = (1.0 - smoothness).clamp(0.04, 1.0)`), `crates/renderer/shaders/triangle.frag` (the `mat.glossMapIndex != 0u` branch)

## Description

BGSM `smoothness == 1.0` lowers to `roughness = 0.04` — the clamp floor. The shader is built
to modulate that back up from the gloss map (`roughness = mix(1.0, roughness, glossTexel.r)`),
but only when `mat.glossMapIndex != 0u`. Materials whose modulating map is missing stay
pinned at 0.04, i.e. near-mirror dielectric.

## Evidence

Current code (verified 2026-08-30):

```rust
let roughness = (1.0 - leaf.smoothness).clamp(0.04, 1.0);
material.roughness_override = Some(roughness);
```

```glsl
if (mat.glossMapIndex != 0u) {
    ...
    } else {
        roughness = mix(1.0, roughness, glossTexel.r);
    }
}
```

MEASURED over the installed FO4 material corpus (9,023 BGSM/BGEM files, **all version 2**,
zero parse failures):

- **6,203 of 8,330 BGSMs (74.5%) author `smoothness == 1.0`** — the Bethesda Material Editor
  default (`MaterialLib/BGSM.cs`, tooltip "Smoothness of the specular effect", with the
  per-texel data living in the `SmoothSpec` slot).
- Of those, **345 author no leaf `smooth_spec_texture`**, and **80 more name a DDS absent
  from all 15 texture archives** → **≤425 materials (5.1%) end at roughness 0.04 with
  nothing to modulate them.**
- Upper bound only: a template parent can still supply the slot via `resolved.walk()`.

Affected materials are concentrated on hair, eyeballs and creature glow.

## Impact

Up to 425 materials render as near-mirror dielectrics. The translation itself faithfully
mirrors the source authoring — the gap is the absence of a neutral fallback for the case
where the modulating map is unavailable.

(This candidate was originally framed as "74.5% of FO4 materials read as mirrors"; that
premise was narrowed to ≤5.1% by measurement before filing, since 94.7% of BGSMs do author a
`smooth_spec` map.)

## Suggested Fix

When `smoothness == 1.0` and no `smooth_spec` role resolves after the full `resolved.walk()`,
fall back to a neutral roughness rather than the 0.04 floor — the same "no modulating map
available" branch the shader already implies.

## Related

#1476 (saturation metalness), #1241 / #1244 (PBR seeding).

## Completeness Checks
- [ ] **SIBLING**: the metalness half of the same merge (`bgsm_metalness`) has its own map-absent case — check it for the same shape
- [ ] **CANONICAL-BOUNDARY**: the fix belongs at the BGSM merge / `translate_material` boundary or in `Material::resolve_pbr`, never as a render-time fallback in `triangle.frag`. See `/audit-nifal`.
- [ ] **TESTS**: a regression test pins one of the 425 measured materials resolving to the neutral value, not 0.04


---

# #3641: precombine LOD tie-break relies on max_by_key's last-wins accident

**Source**: `docs/audits/AUDIT_FO4_2026-08-30.md` — Dimension 1
**Severity**: LOW
**Location**: `byroredux/src/cell_loader/precombined.rs` — `build_precombine_meshes`, the `(0..3).max_by_key(|&(c, _)| c)` LOD selection

## Description

Precombine LOD selection picks the LOD with the most triangles via `max_by_key`. Rust's
`max_by_key` returns the **last** maximum, so on a tie the highest LOD index wins **by
accident** rather than by a stated rule.

## Evidence

Verified 2026-08-30 — `byroredux/src/cell_loader/precombined.rs` carries the
`.max_by_key(|&(c, _)| c)` selection (and a sibling `.max_by_key(|&(count, _)| count)`).

MEASURED over the CSG corpus (46,422 shared-geometry objects decoded, 76,498 placed
instances, zero decode errors):

- **49 of 46,422 objects (0.11%)** have two or more LODs sharing the maximum triangle count.
- Selection distribution: LOD0 wins 28,494, LOD1 7,012, LOD2 10,916 — **38.6% of objects
  have their finest triangulation at index 1 or 2**, so the selection itself is doing real
  work and is not a candidate for simplification.

The 49 ties are alternative triangulations of one surface, so they are visually equivalent.

## Impact

Determinism/intent nit, not a rendering defect: the outcome is correct today but depends on
an unstated property of `max_by_key`, which makes it fragile to a refactor that swaps in
`max_by` or an iterator with different tie semantics.

## Suggested Fix

State the tie-break explicitly (e.g. prefer the lowest LOD index on equal triangle counts, or
document that the highest is intended) so the behaviour survives a refactor.

## Related

`a30c088a` (single-LOD handling), #1590 / #2369 (precombine owner + CSG routing).

## Completeness Checks
- [ ] **SIBLING**: the second `max_by_key` in the same file has the same tie exposure — settle both
- [ ] **TESTS**: a regression test pins the chosen tie-break on a synthetic two-LOD-equal object


---

# #3652: make_billboard_system (PostUpdate) reads camera pose camera_follow_system (Late) authors

Source: `docs/audits/AUDIT_CONCURRENCY_2026-08-30.md` — CONC-D4-2026-08-30-01 (MEDIUM, D4 · Scheduler Access Declarations, cross-stage sequencing).

**Same shape as #3180, which fixed the inversion for `submersion_system` only.**

**Location**: `byroredux/src/boot.rs:1243-1253` (billboard registration) + `byroredux/src/boot.rs:1328-1348` (camera_follow registration); read site `byroredux/src/systems/billboard.rs:63-84`.

## Description

`make_billboard_system` is a `Stage::PostUpdate` **exclusive**; its first act is to read the active camera's `GlobalTransform` and derive `cam_pos` / `cam_forward`, which is the **entire input** to every billboard rotation it writes.

In `PlayerMode::Character` the **sole author** of that camera pose is `camera_follow_system`, registered `add_to_with_access(Stage::Late, ...)` declaring `.writes::<GlobalTransform>()` + `.writes::<Transform>()` (`fly_camera_system` early-returns in Character mode, `systems/camera.rs:20-26`).

`Stage::PostUpdate` (discriminant 2) executes **strictly before** `Stage::Late` (discriminant 4), so within frame N the billboard system reads the pose authored in Late of frame **N-1** — and the `transform_propagation` pass that runs immediately before it in PostUpdate recomposes the camera `GlobalTransform` from that same frame-N-1 `Transform`, so **there is no second path to a fresh value**. The renderer then draws frame N from the frame-N camera pose (`build_render_data` runs after the whole schedule), against billboards oriented to frame N-1.

This is exactly the defect #3180 found and fixed for `submersion_system` — commit `5ce2b1c5` moved that one system from `PostUpdate` to `Late` and **left the sibling PostUpdate consumer of the same camera pose in place**. The #1375 invariant comment directly above the billboard registration (`boot.rs:1220-1236`) reasons only about Late-stage *writes* of `GlobalTransform` versus `WorldBound` propagation; it never considers a PostUpdate *read* of a Late-authored pose.

## Evidence

```rust
// byroredux/src/boot.rs:1243-1253
scheduler.add_exclusive_with_access(
    Stage::PostUpdate,
    make_billboard_system(),
    Access::new()
        .reads_resource::<ActiveCamera>()
        ...
        .writes::<byroredux_core::ecs::GlobalTransform>(),
);

// byroredux/src/systems/billboard.rs:77-85
let Some(mut gq) = world.query_mut::<GlobalTransform>() else { return; };
let Some(cam_global) = gq.get(cam_entity).copied() else { return; };
let cam_pos = cam_global.translation;
let cam_forward = cam_global.rotation * -Vec3::Z;

// byroredux/src/boot.rs:1328-1332  (the sole Character-mode author of that pose)
scheduler.add_to_with_access(
    Stage::Late,
    crate::systems::camera_follow_system,
```

Stage order that makes it structural — `crates/core/src/ecs/scheduler.rs:27-38` (`Early=0 ... PostUpdate=2 ... Late=4`, `BTreeMap` ascending) and `:497-515` (per stage: whole parallel phase, then exclusives).

## Trigger Conditions

`PlayerMode::Character` (the gameplay camera) + any frame in which the camera pose changes. **Not** reachable in `PlayerMode::FlyCam`, where `fly_camera_system` writes the camera `Transform` in `Stage::Early` and `transform_propagation` composes its `GlobalTransform` in the same PostUpdate parallel phase that precedes the billboard exclusive.

## Verification Path

`cargo test` cannot see it — the analyzer only reasons **within** a stage, so `known_conflict_count()` / `unknown_pair_count()` both stay 0 (`analyze_pair` never compares systems in different stages). Confirm by hand from the stage table, or visually: fast mouse-yaw in an exterior in player mode — billboard/impostor quads shear or show a sliver edge that snaps back when the camera stops.

## Impact

**One full frame of camera lag on every billboard rotation in gameplay (player) mode.** At 60 fps and a 400 deg/s flick that is ~6.7 deg of facing error, which for a camera-facing quad is visible as shear/sliver on grass, tree impostors and SpeedTree billboards during fast turns, resolving as soon as the camera stops (the `camera_changed` gate at `billboard.rs:93-96` means the steady state is correct).

No race, no unsoundness — a pure ordering defect, and **invisible to the scheduler KPIs**.

## Related

#3180 (`5ce2b1c5`, the identical inversion for `submersion_system`); #1374 / #1375 (billboard camera-motion gate + the PostUpdate ordering contract); #217 (bounds propagation must run after billboard rotations); CONC-D4-2026-08-30-02 (same class, smaller blast radius).

## Suggested Fix

Move `camera_follow_system` so the pose is authored **before** its PostUpdate consumer — it only needs the player body's *propagated* `GlobalTransform`, so a `Stage::PostUpdate` **exclusive** registered between `transform_propagation` and `make_billboard_system` satisfies every existing contract (billboards see the current pose; bounds propagation, still last, sees the final camera GT; the Late water/audio consumers still sequence after it).

**Note this contradicts** `submersion_runs_after_camera_follow_and_before_water_audio`, which asserts `!late.systems[camera_follow].is_exclusive` — that pin has to be rewritten in the same commit, and the #3180 orderings (camera_follow before submersion before water_audio before audio_system) re-expressed **across** stages rather than within Late.

## Completeness Checks
- [ ] **SIBLING**: Every PostUpdate consumer of a Late-authored value enumerated, not just the billboard one — #3180 fixed one instance of a class
- [ ] **TESTS**: `submersion_runs_after_camera_follow_and_before_water_audio` rewritten in the same commit; the #3180 orderings re-expressed across stages
- [ ] **TESTS**: A regression test pins the cross-stage ordering, since `analyze_pair` is intra-stage and cannot

