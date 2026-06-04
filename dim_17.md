# Renderer Audit — Dimension 17: Water Rendering (M38)

## Scope and Focus
This audit covers the comprehensive evaluation of water rendering mechanisms, specifically focusing on:
- **WaterPlane Spawning**: Component creation from XCWT records, height/extent correctness.
- **Geometry & Physics**: Vertex displacement amplitude bounds, tessellation safety, and prevention of Z-fighting.
- **Optical Properties**: Fresnel term accuracy (Schlick F0 ~0.02 vs glass IOR 1.5), and accuracy of reflection/refraction rays.
- **Caustic Synthesis (#1210)**: Validation of all 5 implementation phases.
- **Submersion State**: Camera crossing detection, underwater fog/tint application, and boundary strobe prevention.
- **Material & Rendering**: Distinct `GpuMaterial` entries, sort key ordering (alpha blend cluster), and shadow casting exclusions.
- **Cleanup Routines**: Cell unload despawn safety and BLAS leak prevention.

## Pipeline Trace & Verifications
- Traced code paths across `water.rs`, `water_caustic.rs`, `water.vert/frag`, `cell_loader/water.rs`, and `systems/water.rs`.
- Verified the water-side caustic implementation aligns with the `caustic_splat` parallel approach from Dimension 13.
- Confirmed material deduplication appropriately distinguishes between water and glass instances (`is_water` flag ensures no overlap).

## Caustic Synthesis (#1210) Status
- **Phase A+B**: `sun_direction` is successfully plumbed through `CameraUBO`.
- **Phase C**: `WaterCausticAccum` image lifecycle correctly implemented as per-frame R32_UINT.
- **Phase D**: Water-side caustic synthesis is actively functioning within `water.frag`.
- **Phase E**: Composite integration of accumulated caustics verified.
No regressions detected across Phases A-E from the #1210 series.

## Conclusion
The Dimension 17 water rendering pipeline remains stable and robust. All water mechanics behave consistently, and the newly integrated #1210 caustic synthesis requirements function as expected without introducing regressions or deduplication issues.