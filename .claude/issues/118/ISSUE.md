# NIF-517: Particle system blocks cause hard parse failure on Oblivion

## Finding

**Audit**: NIF 2026-04-05b | **Severity**: HIGH | **Dimension**: Coverage

**Location**: `crates/nif/src/blocks/mod.rs`
**Game Affected**: Oblivion

### Description

~15 NiPSys* particle types have no parser and are absent from the dispatch table entirely. On games with `block_sizes` (FO3+), unrecognized types are safely skipped via NiUnknown. On Oblivion (v20.0.0.5, no block_sizes), any NIF containing particle effects fails hard with no recovery — the parse loop breaks at the first unrecognized type.

Common Oblivion NIFs with particles: fire effects, magic effects, weather, water splash.

### Suggested Fix

Add NiPSys* types to the dispatch table with minimal parsers that consume the correct byte count per nif.xml, or compute expected sizes from the block type and version.

### Completeness Checks

- [ ] **SIBLING**: Enumerate all NiPSys* types that appear in Oblivion NIFs
- [ ] **TESTS**: Regression test parsing an Oblivion NIF with particle blocks
- [ ] **UNSAFE**: N/A

🤖 Generated with [Claude Code](https://claude.com/claude-code)
