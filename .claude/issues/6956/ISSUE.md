# #6956: TD2-2026-11-15-03: "Prepend `meshes\` unless present" has a single home (`normalize_mesh_path`) and nine hand-spelled copies, five behaviours

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/6956
- **Labels**: low,import-pipeline,tech-debt,bug
- **Source**: docs/audits/AUDIT_TECH_DEBT_2026-11-15.md

From the tech-debt audit `docs/audits/AUDIT_TECH_DEBT_2026-11-15.md` (HEAD 995ed089b).

- **Dimension**: 2 · **Severity**: LOW · **Effort**: small · **Labels**: `tech-debt` · **Existing**: none
- **Age**: `mesh_path` ×3 df6ad0410 / 3bb6806b5 (2026-10-14); `kf_mesh_path` d1b298921 (2026-10-18); others older
- **Location**: the single home is `byroredux/src/asset_provider/archive.rs:695` (`pub fn normalize_mesh_path`, case-insensitive on either separator, and exempts `geometries\` per #1292). The copies:
  - `byroredux/src/asset_provider/object_graph.rs:67`, `archetype_idle.rs:62`, `actor_graph.rs:69` (`fn mesh_path`; the second and third are character-for-character equal, the first differs only by not lower-casing)
  - `byroredux/src/asset_provider/animation.rs:207` (`kf_mesh_path`)
  - `byroredux/src/cell_loader/placement_lod.rs:266` (`full_model_path`), `nif_import_registry.rs:49` (`canonical_model_path_key`), `tree_lod_legacy.rs:264` (`lod_flat_nif_path`), `references/import.rs:358` (`meshes_rooted` closure), `references/synth_child.rs:640` (inline `starts_with("meshes\\") || ...("meshes/")` chain)
  - `project_directory` is also byte-identical in `archetype_idle.rs:74` and `actor_graph.rs:78`.
- **Description**: the helper's own doc says "the cell-loader form is now a redundant idempotent double-normalisation and can be removed in a follow-up sweep"; that sweep never happened, and the copies kept multiplying. They disagree on lower-casing, on `/` versus `\` handling, on whether a leading separator is trimmed, and none carries the #1292 `geometries\` exemption that the home needed (Starfield content-addressed `.mesh` companions, where a `meshes\` prefix made 99.7% of Cydonia statics miss). I found no `geometries\` input that reaches any copy today, so I rate it LOW; it meets the letter of the promotion trigger (one copy fixed, the others not) and should be promoted if such an input is found.
- **Suggested Fix**: replace the nine bodies with `normalize_mesh_path(..)` (plus `.to_ascii_lowercase()` / `replace('/', "\\")` at the three sites that need a lower-cased key, via one `canonical_mesh_key(path) -> String` next to it), move `project_directory` into `asset_provider/` once (it needs `mesh_path`'s prefix rule too), and delete the five private copies. Add a `workspace_hygiene_tests` scan for `format!("meshes\\{` outside `archive.rs`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or an extended guard) pins this specific fix
