# Issues 2232, 2233, 2238, 2239

## #2232 — REN-D6-01: Fire-refraction's `ior` field overload and 8 hand-translated material fields
**Location**: `crates/core/src/ecs/components/material.rs:26` (`SurfaceBehavior::ior`), `crates/renderer/shaders/include/bindings.glsl:38` (`GpuMaterial.ior`), `triangle.frag` fire-refraction branch, `byroredux/src/material_translate.rs`, NIF import material walker.

`GpuMaterial.ior` has 3 meanings: ordinary Fresnel IOR, glass IOR via SurfaceBehavior, and (undocumented) fire-refraction 0-1 distortion scalar (`MATERIAL_KIND_FIRE_REFRACTION`). Need doc notes at both `ior` field sites. Separately (pre-existing, larger scope): 8 hand-translated material fields outside NIFAL boundary — deferred/tracked only, not required for this fix.

**Fix scope (this session)**: add doc comments at `bindings.glsl:38` and `material.rs:26` enumerating all 3 discriminated meanings by `materialKind`.

## #2233 — REN-D8-02: composite.frag's is_sky branch skips bloom and volumetric/height-fog term
**Location**: `crates/renderer/shaders/composite.frag`, `is_sky` branch (~394-397), bloom + volumetric fog terms gated inside `has_surface` branch (~480+).

Sky pixels get no bloom and no volumetric/height-fog contribution, causing a visible seam at the horizon. Fix: restructure so bloom + volumetric fog terms are computed once and applied on both branches.

## #2238 — REN-D14-01: MultiLayerParallax is a caustic source but never enters SHADOW_MASK_GLASS
**Location**: `crates/renderer/shaders/caustic_splat.comp:13` (comment), `crates/renderer/src/vulkan/acceleration/predicates.rs:594-614` (`shadow_mask_for_instance`).

`shadow_mask_for_instance` only assigns `SHADOW_MASK_GLASS` to literal `MATERIAL_KIND_GLASS` (100); MultiLayerParallax (`material_kind == 11`) falls into `SHADOW_MASK_OPAQUE`, causing self-illumination artifacts. Fix: add MLP to the `SHADOW_MASK_GLASS` branch.

## #2239 — REN-D14-02: Parked-camera caustic EMA truncates dim caustics toward zero via fixed-point atomic underflow
**Location**: `crates/renderer/shaders/caustic_splat.comp`, `emaWeight = 1.0 - pc.decayFactor;` + fixed-point `imageAtomicAdd` deposit (~line 502-520).

Same bug class as #1942 (sun path): `contrib * emaWeight` can round below 1 fixed-point ULP under a parked camera, causing dim caustics to decay to zero instead of converging. Fix: apply same pattern as #1942 fix.
