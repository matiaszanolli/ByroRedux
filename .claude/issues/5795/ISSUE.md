# TD2-2026-10-15-04: `CachedNifImport` is assembled twice with a mirrored 60-line scene-satellite extraction; the streaming copy lagged on two fields (#3072/#3074) and M47.2 has just pasted a third block into both

Labels: medium,import-pipeline,tech-debt,bug

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD2-2026-10-15-04`).

- **Severity**: MEDIUM
- **Dimension**: 2 - Logic Duplication
- **Location**: `byroredux/src/cell_loader/partial.rs:60-200` (streaming partial, build at `:158`) vs `byroredux/src/cell_loader/references/import.rs:45-270` (`parse_and_import_nif`, build at `:193`); `import.rs:520` is the deliberate SpeedTree shell and is not part of this
- **Status**: NEW
- **Age**: `8209a03f9` (#3072/#3074, 2026-09-02: furniture markers and flame-attach offset hard-coded `None` on the partial path citing a blocker that was false); M47.2 `graph_nodes`/`behavior_graph_file` block (2026-10-13/14) pasted verbatim into both
- **Effort**: small
- **Description**: both functions run `collision_authoring`, `find_attach_light_offset`, `find_projectile_node_offset`, `extract_furniture_markers` -> `furniture_component`, `extract_behavior_graph_file` -> `graph_nodes::build` -> `import_kf`, `extract_phantom_bounds`, the post-merge `merge_external_materials` snapshot and the embedded-clip registration, then spell the same 25-field struct literal. The partial path's comments cite the sync path ("computed the same way the synchronous `parse_and_import_nif` path does", "mirroring the synchronous path (`references/import.rs`)") three times.
- **Evidence**: `8209a03f9` message: the partial path "used to hardcode `None`, citing the same (false) blocker" for `attach_light_offset` and `furniture`; the identical 5-line `behavior_graph_file`/`graph_nodes`/`graph_sequences` block now sits at `partial.rs:150-156` and `import.rs:182-190` (clone scan: 9 identical windows).
- **Impact**: cell-streamed REFRs (every exterior cell) take the partial path; a satellite added to only the sync path (loose NIF, interior) is silently absent from streamed cells, which is precisely the #3072 failure.
- **Related**: #3072, #3074, #1594 (the one deliberate asymmetry: `attach_points`/`child_attach_connections` are `None` on partial), #4290.
- **Suggested Fix**: add `fn scene_satellites(scene: &NifScene, meshes: &[ImportedMesh], collisions: &[ImportedCollision]) -> SceneSatellites { collision_authoring, attach_light_offset, projectile_offset, furniture, behavior_graph_file, graph_nodes, graph_sequences, phantom_bounds }` in `nif_import_registry.rs` next to `CachedNifImport`, and `CachedNifImport::from_scene(...)` taking it; the partial path overrides only `attach_points: None` with the #1594 comment.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
- [ ] **TESTS**: A regression test or guard pins this specific fix (or the existing guard is named)
