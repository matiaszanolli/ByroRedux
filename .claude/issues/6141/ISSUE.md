# TD1-2026-10-19-04: `crates/nif/src/import/types.rs` sits on the line (2001 production LOC) and mixes eight type families plus five inline test modules

Labels: low,nif-parser,tech-debt,bug
Source: docs/audits/AUDIT_TECH_DEBT_2026-10-19.md

- **Severity**: LOW
- **Dimension**: 1 - File / Function / Module Complexity
- **Location**: `crates/nif/src/import/types.rs:1-2442` (tests inline at L554, L1179, L1835, L2294, L2355)
- **Status**: NEW (watch-listed 1997 on 10-15; 44% comments)
- **Effort**: medium
- **Description**: pure data + small accessors, no function over ~130 lines, so the cost is navigation and merge collisions (every M47.2/FO4 import field lands in `ImportedMaterial`/`ImportedMesh`/`ImportedScene`). Families: lights/texture effects/collision/nodes L31-300; `MaterialTextureSet<T>` + impl + tests L326-657; `ImportedMaterial` + Default L658-959; `ImportedMesh`, morphs, skin, bones, dismember helpers + impls L960-1575; attach points/furniture/`ImportedScene` + `Similarity`/`compose_geometry_bounds` L1576-1908; ragdoll L1909-2028; particle emitters + curves + flat form L2029-2293.
- **Evidence**: `prod_loc` 2001; `grep -nE '^(pub )?(struct|enum|impl|fn|mod)'`.
- **Proposed fix**: `import/types/{mod.rs (re-exports), light.rs, node.rs, material.rs (+MaterialTextureSet tests), mesh.rs (mesh/skin/bone + skin_partition tests), scene.rs (+geometry_bounds tests), ragdoll.rs, particles.rs (+role_enumeration tests)}` with `pub use` in `types/mod.rs` so every `super::types::X` caller is unchanged. `include_str!("types.rs")` has no reader in `crates/nif` (checked); `crates/core/src/ecs/components/material.rs:757, 840` cite the path in comments, refresh them.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the related files named above
- [ ] **TESTS**: A regression test / source-scan guard pins this specific fix (or the finding is a pure move/delete covered by existing tests)

_Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-19.md` (HEAD 12677d718). Fix with `/fix-issue <number>`._
