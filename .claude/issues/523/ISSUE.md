# FNV-CELL-3: NifImportRegistry write-lock churn per REFR

- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/523
- **Severity**: MEDIUM
- **Dimension**: Cell loading / performance
- **Audit**: `docs/audits/AUDIT_FNV_2026-04-21.md`
- **Status**: NEW (created 2026-04-21)

## Location

`byroredux/src/cell_loader.rs:1177-1211` (inside per-REFR loop at :1053)

## Summary

Cache hit path acquires `world.resource_mut::<NifImportRegistry>()` once per REFR. 809 write-lock/release cycles on Prospector Saloon; scales linearly. No actual contention (single-threaded), but ~100µs/cell wasted on lock cycles.

Fix: batch REFRs into a pre-flight pass; acquire lock once for batch hit-check, once for batch insert after parsing.

Fix with: `/fix-issue 523`
