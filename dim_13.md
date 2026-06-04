# Renderer Audit — Dimension 13: Caustics and Glass Materials

## Scope and Focus
This audit covers the comprehensive evaluation of caustics and glass rendering mechanisms, specifically focusing on:
- **Glass Optical Properties**: IOR-driven Fresnel F0 derivation (`dielectricF0FromIor`), refraction with roughness spread (Frisvad orthonormal basis), and reflection rays.
- **Ray Budgeting**: Validation of the `GLASS_RAY_BUDGET` allocation (currently at 8192) and fallback paths when the budget is exhausted.
- **Infinite Loop Prevention**: Safety checks against glass-passthrough infinite loops during IOR refraction.
- **Caustic Synthesis (#1210)**: Validation of the `caustic_splat.comp` parallel implementation, including instance bounds checks.
- **Material Classification**: Glass material flag assignments, deduplication logic, and isolation of `env_map_scale` from metalness on dielectric surfaces.

## Pipeline Trace & Verifications
- Traced code paths across `triangle.frag`, `caustic_splat.comp`, `composite.frag`, and `crates/renderer/src/vulkan/material.rs`.
- Verified the `dielectricF0FromIor` implementation clamps its input domain correctly.
- Confirmed the fix for the glass-passthrough infinite loop using a texture-equality identity check (#789) remains intact.
- Verified the Frisvad orthonormal basis is used for IOR refraction roughness spread (#820), avoiding NaN generation on vertical surfaces.
- Checked `caustic_splat` instance bounds are enforced against the R16_UINT ceiling (#738).

## Caustic Synthesis & Glass Status
- **Glass Shading**: `dielectricF0FromIor(eta)` correctly replaces the hardcoded F0, providing accurate Fresnel response for glass (IOR ~1.45-1.5).
- **Refraction Ray**: Fires along the geometric normal with IGN-sampled roughness spread. Sky-tint fallback has been correctly replaced with cell-ambient for interiors.
- **Caustic Splatting**: `caustic_splat` instance buffer reads correctly index the `avg_albedo` field (which was explicitly preserved for caustics during the R1 MaterialTable refactor).
- **Material Deduplication**: Glass instances are correctly deduplicated in `MaterialBuffer` without cross-contamination.

## Conclusion
The Dimension 13 caustics and glass rendering pipeline is stable. The IOR-based glass shading and caustic splat mechanisms behave correctly. Previous issues such as the refraction infinite loop and vertical surface NaNs have been successfully resolved and exhibit no regressions.