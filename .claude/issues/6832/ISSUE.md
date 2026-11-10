# REN-D8-10: misattached doc blocks in `fog_clusters.rs`; `shader-pipeline.md` lacks the beyond-grid tail / exterior air-fade description

Labels: low,renderer,doc-rot,documentation
Filed from: volumetrics-deep audit suite, 2026-11-09

**Severity**: LOW (doc-rot). **Status**: NEW.

**Location**: `crates/renderer/src/vulkan/volumetrics/fog_clusters.rs`; `docs/engine/shader-pipeline.md`.

## Description / Evidence
- The "whether `transportCombustion`'s RK2 advection block ... should run" paragraph (~L99) sits on `TRANSPORT_EXPIRED_DT`, leaving `combustion_transport_active` undocumented.
- The `build_fog_volume_clusters` "#2242 ... every call resets every entry's `count`" paragraph (~L254) and the "Conservative world-space box" paragraph (~L267) sit on `fog_volume_is_shape`.
- `grep -n -i "beyond-grid\|analytic tail\|sky_air" docs/engine/shader-pipeline.md` returns nothing: the tail, its mean-density scaling and the exterior `sky_air_terms` fade added by #6728 are undocumented there (prose lives only in `docs/prd/volumetric-lighting.md` and `docs/engine/skyal.md` section 2.4 for the cloud half).

## Impact
None at runtime; a reader of `build_fog_volume_clusters` loses its reset-every-call contract, which is the stale-count guard.

## Suggested Fix
Move the three paragraphs onto the items they describe; add one composite-step bullet to `shader-pipeline.md`.

Report: `docs/audits/AUDIT_RENDERER_2026-11-09.md`.
