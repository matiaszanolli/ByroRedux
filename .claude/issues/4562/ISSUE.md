# NIFAL-D6-2026-09-21-01: Havok rename missed one synthesize_packed_havok_proxy reference in physics/convert.rs

**Labels**: low, physics, documentation, doc-rot

**Severity**: LOW · **Dimension**: Collision · **Tier Violated**: — (doc-rot introduced by the rename) · **Game Affected**: FO4 / FO76 / Starfield (packed-collision proxy path)
**Location**: `crates/physics/src/convert.rs:239`
**Source**: `docs/audits/AUDIT_NIFAL_2026-09-21.md` (CONFIRMED at `4dca737e2`)

### Description
`4dca737e2` renamed `synthesize_packed_havok_proxy` → `synthesize_packed_collision_proxy` in `cell_loader/spawn.rs` but left the #2543 comment in the Cuboid arm of the canonical→Rapier converter citing the old name — the repo's only remaining `packed_havok` hit.

### Evidence
`grep -rn "packed_havok" crates byroredux` → exactly one hit, `convert.rs:239`.

### Impact
Documentation/search only; a reader grepping the renamed symbol won't find the cross-reference.

### Related
#2543 (context), #4408 (adjacent stale-comment lead in the same area, already open)

### Suggested Fix
One-word comment edit to `synthesize_packed_collision_proxy`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other readers, other gates)
- [ ] **TESTS**: A regression test pins this specific fix
