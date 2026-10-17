# REN-VOL-L-03: doc rot — the per-volume field descriptions of the combustion textures disagree with the shader

Labels: low,renderer,documentation,doc-rot
Source: docs/audits/AUDIT_RENDERER_2026-10-17.md

- **Severity**: LOW (doc rot)
- **Dimension**: Volumetrics
- **Location**: `docs/engine/memory-budget.md` Volumetrics table rows `combustion_state_volumes` ("σ_a") and `combustion_dynamics_volumes` ("velocity xyz + σ_s"); `volumetrics.rs` doc on `COMBUSTION_FIELD_FORMAT` ("absorption sigma_a", "scattering sigma_s in w"); `memory-budget.md` host-buffer table (omits the two 16 B `integration_param_buffers`, 32 B total — negligible)
- **Status**: NEW
- **Evidence**: `volumetrics_inject.comp` header, `combustionMedium` and `sanitizedChemistry` store total extinction σ_t in `chemistry.z`; dynamics `.w` is specific overpressure (m²/s²); spectral σ_s lives in the separate `combustionOptical` volume. `shader-pipeline.md` rows 14–23 and the `VolumetricsPipeline` field docs are already correct.
- **Suggested Fix**: correct the two table rows and the const doc to match `shader-pipeline.md`.

_Filed from `docs/audits/AUDIT_RENDERER_2026-10-17.md` (area-scoped `volumetrics-deep` suite run, HEAD e16a624e9). Fix with `/fix-issue <number>`._
