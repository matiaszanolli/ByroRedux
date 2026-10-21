# #6271: TD1-2026-10-21-01: `cell_loader/exterior.rs` grew +227 production LOC in two days and is the next 2000-line file

**Labels**: low,terrain-exterior,tech-debt,bug

**Source**: docs/audits/AUDIT_TECH_DEBT_2026-10-21.md · **Severity**: LOW · **Dimension**: 1 · **Location**: `byroredux/src/cell_loader/exterior.rs` (2669 total, 1687 prod); siblings `byroredux/src/cell_loader/terrain.rs:1000` (`spawn_terrain_mesh`, 535 lines), `byroredux/src/cell_loader/reference_state.rs` (1117 prod)

- **Severity**: LOW
- **Dimension**: 1
- **Location**: `byroredux/src/cell_loader/exterior.rs` (2669 total, 1687 prod); siblings `byroredux/src/cell_loader/terrain.rs:1000` (`spawn_terrain_mesh`, 535 lines), `byroredux/src/cell_loader/reference_state.rs` (1117 prod)
- **Status**: NEW (watch-list escalation; not in #6139–#6145)
- **Age**: 11 commits since 12677d718 (`ca12e5638` #6076, `611769b16` #6071–4, `2efed9843` #6102, `67e724c3e` #6127, `588c87bc4` #6123, `3b9060701` #6085, `79477b4a3` #6043, `e62e258bd` #6199, `c53d87679` #6048, `17bd40f4f` #5757, `dc829fc83` #6132)
- **Effort**: medium
- **Description**: The file has three jobs interleaved with five inline test modules:
  - the persistent-cell window and logical actor stubs (`PersistentCellApplyJob` L227, `resolve_persistent_cell` L500, `begin_worldspace_persistent_cell` L1132, `PersistentCellWindow` L1201, `ensure_persistent_actor_stubs` L1293, tile helpers L1336–1366);
  - the streaming `ExteriorCellApplyJob` (L1451, impl L2160–2535, whose `begin` is 207 lines and `advance` 152);
  - worldspace selection and context (`select_worldspace_key` L1617, `build_exterior_world_context` L1959, `load_one_exterior_cell` L2119).

  At the window's rate it crosses 2000 within a week. In `terrain.rs`, `spawn_terrain_mesh` keeps four phases inline: vertex build L1066–1160, base texture resolve L1211–1264, ground-cover density inputs L1281–1330, then tile allocation and spawn.
- **Evidence**: `prod_loc` at base vs HEAD: 1460→1687, 1338→1538, 842→1117. fnscan: `spawn_terrain_mesh` 535, `ExteriorCellApplyJob::begin` 207.
- **Impact**: This is the file every exterior streaming fix touches. Eleven fixes in two days all edited it, and the interleaved test modules make each diff harder to review.
- **Related**: #6145 (`cell_loader.rs` facade), SCR-2026-10-21-05 (`PersistentCellApplyJob` bug, scripting-owned).
- **Suggested Fix**: Split along the three jobs above. Make the test modules siblings (`exterior/persistent_tests.rs` …). Re-point any `include_str!("exterior.rs")` source scans in the same commit (`grep -rn 'exterior.rs"' byroredux/src`). Extract `spawn_terrain_mesh`'s four phases as named helpers.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix

_Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-21.md` (HEAD 4a3e89d56). Fix with `/fix-issue <number>`._
