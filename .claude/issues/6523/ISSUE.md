# TD3-2026-10-27-01: 10 `file.rs:LINE` references name in-tree `.rs` files that splits deleted; #6167's gate skips them

Labels: low, documentation, doc-rot, tech-debt, test-gap
Source: docs/audits/AUDIT_TECH_DEBT_2026-10-27.md

# TD3-2026-10-27-01: 10 `file.rs:LINE` references name in-tree `.rs` files that splits deleted; #6167's gate skips them

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-10-27.md` finding `TD3-2026-10-27-01` (severity LOW, status NEW, CONFIRMED against HEAD `fa6be4a6b`).
**Location (re-mapped to current paths)**: Guard: `crates/core/tests/stale_line_references.rs` (the `let Some([target]) = sources.get(base) ... else { continue }` skip). Sites: `byroredux/src/cell_loader/transition.rs:678`, `byroredux/src/cell_loader/spawn.rs:1151`, `byroredux/src/scene/nif_loader.rs:378`, `byroredux/src/asset_provider/tests/material_path.rs:67`, `docs/engine/m47-0-design.md:26` (all `references.rs:N`, now `cell_loader/references/`); `crates/nif/src/blocks/particle.rs:1110`, `particle_tests.rs:462`, `tri_shape_skin_vertex_tests.rs:1212` (`tri_shape.rs:N`, now `blocks/tri_shape/`); `crates/renderer/src/mesh.rs:28`, `crates/nif/src/import/material/shader_data.rs:123` (`scene_buffer.rs:N`, now `scene_buffer/`).

## Description
The gate skips any basename that resolves to no file. A reference into a file that a split deleted is exactly the case where the line number now means nothing - and exactly the one the gate cannot see. Ten live references exist at HEAD. Legitimate exclusions: five rapier upstream `control/character_controller.rs` refs (`systems/character.rs`, `locomotion.rs`, physics `world/queries.rs`), upstream paths in `dependency-update-followups-plan.md`, and the test's own fixture literals.

## Evidence
Verified by grep at HEAD: e.g. `transition.rs:678` "(`references.rs:198-202`)", `spawn.rs:1151` "`references.rs:840`", `particle.rs:1110` "see tri_shape.rs:108", `shader_data.rs:123` "(scene_buffer.rs:240-263)", `mesh.rs:28` "`scene_buffer.rs:1326+`". `crates/renderer/src/vulkan/scene_buffer/` and `crates/nif/src/blocks/tri_shape/` and `byroredux/src/cell_loader/references/` are directories now.

## Impact
Comments point readers at line numbers in files that no longer exist; the guard that was added for this class (closed #6167) silently passes them.

## Related
#6167 (closed; this is a blind spot of its guard).

## Suggested Fix
Replace the 10 line numbers with symbol + module. Then make the gate fail on a zero-match basename unless the reference carries an upstream-crate prefix or sits on a short allowlist. Add a test with a fixture reference to a split-away file.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / sibling call sites
- [ ] **TESTS**: A regression test pins this specific fix

