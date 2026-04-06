# NIF-206: NiSkinPartition SSE features gated on exact bsver==100

## Finding

**Audit**: NIF 2026-04-05b | **Severity**: LOW | **Dimension**: Version Handling

**Location**: `crates/nif/src/blocks/skin.rs:185,302`
**Game Affected**: SkyrimSE (hypothetical BSVER 101-129)

`detect()` maps BSVER 101-129 to SkyrimSE, but SSE skin partition fields are gated on `bsver==100` exactly. Broaden to `bsver >= 100 && bsver < 130`.

### Completeness Checks
- [ ] **SIBLING**: Check all `bsver == 100` checks in the codebase
- [ ] **TESTS**: Test with BSVER=105

🤖 Generated with [Claude Code](https://claude.com/claude-code)
