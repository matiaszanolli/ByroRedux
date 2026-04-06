# NIF-513: bhkNPCollisionObject skip-only — all FO4 collision lost

## Finding

**Audit**: NIF 2026-04-05b | **Severity**: MEDIUM | **Dimension**: Coverage

**Location**: `crates/nif/src/blocks/mod.rs` (skip-only entry)
**Game Affected**: FO4+

### Description

FO4 uses bhkNPCollisionObject (new physics system, replacing the classic bhk chain). Skip-only means all FO4 collision data is lost. The NP system uses a different block hierarchy than classic bhk.

### Completeness Checks

- [ ] **TESTS**: Parse a FO4 NIF with NP collision
- [ ] **UNSAFE**: N/A

🤖 Generated with [Claude Code](https://claude.com/claude-code)
