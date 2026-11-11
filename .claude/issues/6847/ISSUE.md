# FO4-2026-11-10-D2-03: stale source-path references in BGSM-merge comments

Labels: low,documentation,doc-rot,import-pipeline,game:fo4,legacy-compat
Source: docs/audits/AUDIT_FO4_2026-11-10.md

- **Severity**: LOW
- **Dimension**: 2 (doc rot)
- **Location**: `crates/bgsm/src/bgsm.rs:39-41` (cites `byroredux/src/asset_provider/material.rs` for `MaterialTextureSet`, which lives in `crates/nif/src/import/types/material.rs`; the `.rs` file was split into `material/` by #3857); `byroredux/src/asset_provider/material/merge.rs:1645-1647` ("`pack_imported_material_flags` in `cell_loader.rs`", now `byroredux/src/material_flags.rs` since #6712); `byroredux/src/material_translate.rs:857` ("`asset_provider/material.rs`'s inventory of BGSM fields", now the `merge_bgsm_arm` ledger in `material/merge.rs`); `byroredux/src/material_flags.rs:13,288`; `bgsm_merge.rs:889,2090`.
- **Status**: NEW
- **Evidence**: `ls byroredux/src/asset_provider/material.rs` fails; `git log -1 -- byroredux/src/cell_loader.rs` is not a packer location. `df17d9dba` moved the packers and updated the `material_translate.rs` rustdoc links but left these prose references.
- **Impact**: navigation only; `bgsm.rs:40` is on a public field's doc, so it is the one a parser-crate reader will follow.
- **Suggested Fix**: replace the paths (the ones in `bgsm.rs` and `merge.rs` first).
