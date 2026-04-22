# FNV-CELL-6: unload_cell texture ref-counting gap (WATCH for M40)

- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/524
- **Severity**: MEDIUM (WATCH-item — blocks M40)
- **Dimension**: Cell loading / memory
- **Audit**: `docs/audits/AUDIT_FNV_2026-04-21.md`
- **Status**: NEW (created 2026-04-21)

## Location

- `byroredux/src/cell_loader.rs:163-265`
- `crates/renderer/src/texture_registry.rs:354-400`

## Summary

`drop_texture` unconditionally frees. Shared textures across still-live cells fall back to checkerboard. M40 doorwalking will silently degrade cross-cell clutter.

Fix: add `ref_count: u32` to `TextureEntry`; bump on resolve, decrement on drop, free only at 0. Must land with or before M40.

Fix with: `/fix-issue 524`
