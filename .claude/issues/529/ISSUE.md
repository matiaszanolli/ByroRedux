# FNV-CELL-5: Cloud tile_scale hardcoded to 0.15

- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/529
- **Severity**: LOW
- **Dimension**: Cell loading
- **Audit**: `docs/audits/AUDIT_FNV_2026-04-21.md`
- **Status**: NEW (created 2026-04-21)

## Location

`byroredux/src/scene.rs:204-205`

## Summary

`cloud_tile_scale` is hardcoded regardless of WTHR authored scale. Cosmetic — all clouds tile identically even when different WTHR entries ship different authored densities.

Fix: parse WTHR `cloud_scales`, route to `SkyParamsRes.cloud_tile_scale`, fall back to 0.15.

Fix with: `/fix-issue 529`
