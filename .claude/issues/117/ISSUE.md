# NIF-515: Havok constraint skip-only types cause cascading failure on Oblivion

## Finding

**Audit**: NIF 2026-04-05b | **Severity**: HIGH | **Dimension**: Coverage

**Location**: `crates/nif/src/blocks/mod.rs` (skip-only entries)
**Game Affected**: Oblivion

### Description

7 Havok constraint types (bhkRagdollConstraint, bhkLimitedHingeConstraint, bhkMalleableConstraint, bhkStiffSpringConstraint, bhkBallAndSocketConstraint, bhkBallSocketConstraintChain, bhkHingeConstraint) are skip-only via NiUnknown. When `block_size` is `None` (Oblivion v20.0.0.5), `NiUnknown::parse` returns `Err`, breaking the parse loop. Since collision blocks typically appear before geometry in the NIF block list, all geometry after the first unrecognized constraint is unreachable.

### Suggested Fix

Implement minimal byte-exact parsers for the 7 constraint types (read fields per nif.xml and store as opaque data), or detect Oblivion v20.0.0.5 and skip collision subgraph references entirely during walk.

### Completeness Checks

- [ ] **SIBLING**: All 7 constraint types handled
- [ ] **TESTS**: Regression test with synthetic Oblivion NIF containing a constraint block
- [ ] **UNSAFE**: N/A

🤖 Generated with [Claude Code](https://claude.com/claude-code)
