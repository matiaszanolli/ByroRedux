# #6318: REN-L-02: `shader-pipeline.md`'s `GpuInstance.flags` table omits bit 10 (`INSTANCE_FLAG_MOVED_THIS_FRAME`) and misdescribes bit 9

**Labels**: low,renderer,documentation,doc-rot

**Source**: docs/audits/AUDIT_RENDERER_2026-10-21.md · **Severity**: LOW · **Dimension**: GPU-Struct Layout (docs) · **Location**: `docs/engine/shader-pipeline.md (GpuInstance flags table)`

- **Location**: `docs/engine/shader-pipeline.md`, the flag table under the `GpuInstance` layout
- **Status**: NEW
- **Description**:
  - Bit 10 was added by `3934f46d7` and has no row.
  - Bit 9's row says "Exclude this draw from the RT LOD path". The only shader reader is the `triangle.frag` debug-view `lodBlock` colour, and `scene_buffer/constants.rs` calls it "Raster-only diagnostic state".
- **Suggested Fix**: add the bit 10 row (set for a rigid matrix change or a dirty pose, read on the selected shadow ray's hit) and correct bit 9.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
