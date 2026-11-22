# #7145: PAR-D6-2026-11-20-05: `mesh_cache_key` is a sixth "prepend `meshes/` unless present", outside the scan that guards the other five

**Labels**: low,import-pipeline,tech-debt,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/7145

**Source**: `docs/audits/AUDIT_PARSERS_2026-11-20.md`, finding PAR-D6-05 (`/audit-parsers`, HEAD f3713bf5e)

- **Severity**: LOW
- **Dimension**: I/O & Paths
- **Trigger Input**: any model path reaching `MeshRegistry` (e.g. Starfield `geometries\<hash>.mesh`)
- **Location**: `crates/renderer/src/mesh.rs:84-92`; scan scope `byroredux/src/workspace_hygiene_tests.rs:1262-1284`
- **Status**: NEW (hygiene; no wrong lookup demonstrated)
- **Description**: #6956 made `normalize_mesh_path` the one home of the prefix rule, including the #1292 `geometries\` exemption. `mesh_cache_key` re-implements it (lower-case, `\`->`/`, prefix `meshes/`) without the exemption, so a `geometries\x.mesh` path is keyed `meshes/geometries/x.mesh`. The renderer cannot call the binary's function, and the key is internal to the cache, so every producer of one input gets one key and nothing breaks today. The scan only walks `byroredux/src` and only matches the `"meshes\\{ident}"` spelling, so it could not have caught this copy and would not catch `format!("meshes\\{}", x)`.
- **Impact**: none now; a second producer that keys the same mesh from `canonical_mesh_key` output would split the cache entry for exactly the paths #1292 exempted.
- **Related**: #6956, #1292
- **Suggested Fix**: move the prefix rule (or just the `meshes`/`geometries` predicate) to a crate both can reach (`byroredux_bsa` already hosts `normalize_path`), call it from `mesh_cache_key`, and widen the scan to `crates/*/src` and the positional spelling.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other Bink codec arm, the other reader that shares this helper)
- [ ] **TESTS**: A regression test pins this specific fix
