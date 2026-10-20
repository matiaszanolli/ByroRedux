# TD2-2026-10-19-04: The weak `#[cfg(test)]` cut is back at 9 sites the guard cannot see, and ~11 more re-spell `production_text` inline (prior-partial: TD2-2026-10-15-23, closed #5824) (regression of #5824)

Labels: low,tech-debt,bug
Source: docs/audits/AUDIT_TECH_DEBT_2026-10-19.md

- **Severity**: LOW (second recurrence, carried)
- **Dimension**: 2 - Logic Duplication
- **Location**: weak, newline-prefixed (truncates at a `#[cfg(test)] fn`/`use`): `crates/debug-server/src/evaluator.rs:986`, `crates/physics/src/water.rs:3919`, `crates/renderer/src/vulkan/context/build_and_upload_instances.rs:2047`, `byroredux/src/app_step.rs:1726,1749`, `byroredux/src/cell_loader/spawn/mesh_instance.rs:1761,1792,1843,1946`. Strong needle spelled inline instead of calling `source_scan`: `crates/ui/src/prepare.rs:1004` (ui does not depend on core), `crates/physics/src/sync.rs:2684`, `physics/src/world/mod.rs:1715`, `renderer/src/vulkan/context/resources.rs:815,866,911`, `byroredux/src/systems/cinematic.rs:2272`, `systems/combat_ai.rs:689`, `boot/schedule/mod.rs:810`, `material_translate.rs:2840,2887`, `npc_spawn/tests.rs:2909`.
- **Age**: #5824 (8bd4f3b8b, 2026-10-15) migrated 18 sites; two of them (`cinematic.rs:2272`, `combat_ai.rs:689`) were rewritten to the inline strong needle rather than to `production_text`
- **Effort**: small
- **Description**: `no_source_scan_cuts_with_the_weak_cfg_test_needle` (`workspace_hygiene_tests.rs:708`) lists `find("#[cfg(test)]")`, `rfind`, `split`, `split_once` with and without a leading `\n` — but not `split("\n#[cfg(test)]")` / `find("\n#[cfg(test)]")`, which is the spelling of the nine weak sites. The strong inline copies are not weak but break the one-home rule (`crates/core/src/source_scan.rs:22,36`).
- **Proposed fix**: add the two newline-prefixed needles to the guard (and let it also flag an inline `"\n#[cfg(test)]\nmod "` outside `source_scan.rs` plus the `ui`/`pex`/`facegen` allowlist), then migrate the 9 weak sites to `production_text_or_whole` and the in-workspace strong copies to `production_text`. For `byroredux-ui`, either allowlist it or add the already-approved-style dev-dependency.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the related files named above
- [ ] **TESTS**: A regression test / source-scan guard pins this specific fix (or the finding is a pure move/delete covered by existing tests)

_Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-19.md` (HEAD 12677d718). Fix with `/fix-issue <number>`._
