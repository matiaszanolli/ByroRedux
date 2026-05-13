# #995 — SPT-D4-02: `bs_bound` not Z-up → Y-up converted in SpeedTree importer

- **Severity**: MEDIUM
- **Domain**: legacy-compat / import-pipeline
- **Audit**: `docs/audits/AUDIT_SPEEDTREE_2026-05-13.md`
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/995

## TL;DR
TREE.OBND is Bethesda Z-up. The NIF importer applies the axis swap at `crates/nif/src/import/mod.rs:208-211` when hoisting `bs_bound`. The SpeedTree importer copies OBND through raw, so the bounds are in Z-up while the placeholder mesh is in Y-up. Wide-and-shallow AABB instead of tall.

## Sites
- `crates/spt/src/import/mod.rs:124-137` — raw copy, no swap
- `crates/nif/src/import/mod.rs:208-211` — reference: correct swap

## Fix direction
Apply the same axis swap. Promote `zup_point_to_yup` from `crates/nif/src/import/coord.rs` to a shared module so SPT/NIF can't drift again.

## Status
Currently masked by #994 (which discards `bs_bound` via `CachedNifImport`). Fix together.
