# NIF-508: bhkCompressedMeshShape skip-only — majority of Skyrim collision lost

## Finding

**Audit**: NIF 2026-04-05b | **Severity**: MEDIUM | **Dimension**: Coverage

**Location**: `crates/nif/src/blocks/mod.rs` (skip-only entry)
**Game Affected**: Skyrim+

### Description

bhkCompressedMeshShape (and bhkCompressedMeshShapeData) are the primary collision format for Skyrim, replacing the older bhkMoppBvTreeShape + bhkPackedNiTriStripsShape combination. Currently skip-only, meaning most Skyrim collision data is silently discarded.

### Completeness Checks

- [ ] **SIBLING**: Also implement bhkCompressedMeshShapeData
- [ ] **TESTS**: Parse a Skyrim NIF with compressed mesh collision
- [ ] **UNSAFE**: N/A

🤖 Generated with [Claude Code](https://claude.com/claude-code)
