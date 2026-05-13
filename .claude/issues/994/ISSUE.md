# #994 — SPT-D4-01: Cell-loader placeholder loses `Billboard`

- **Severity**: HIGH
- **Domain**: legacy-compat / import-pipeline
- **Audit**: `docs/audits/AUDIT_SPEEDTREE_2026-05-13.md`
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/994

## TL;DR
The SpeedTree importer correctly authors `ImportedNode { billboard_mode: Some(BsRotateAboutUp) }`. The loose `--tree` CLI path consumes that field; the cell-loader path doesn't, because `CachedNifImport` drops `nodes`. Result: every `.spt`-routed REFR in a loaded cell spawns as a static quad instead of a yaw-billboard. Hidden in tests because the CLI path works; broken in-engine.

## Sites
- `crates/spt/src/import/mod.rs:169` — author site
- `byroredux/src/scene/nif_loader.rs:391-393` — loose-NIF path consumes it (works)
- `byroredux/src/cell_loader/references.rs:936-947` — cache adapter drops `imported.nodes`
- `byroredux/src/cell_loader/nif_import_registry.rs:34-59` — `CachedNifImport` struct

## Fix direction
Extend `CachedNifImport` with `billboard_modes: Vec<(usize, BillboardMode)>` and re-insert at `spawn_placed_instances`. Also unblocks `NiBillboardNode`-rooted Skyrim+ meshes.

## Bundle with
- #995 (SPT-D4-02 `bs_bound` Z-up) — currently masked by this issue.
