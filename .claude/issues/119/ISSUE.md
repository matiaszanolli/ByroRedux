# NIF-302: NiTexturingProperty shader texture entries missing has_texture_transform

## Finding

**Audit**: NIF 2026-04-05b | **Severity**: MEDIUM | **Dimension**: Stream Position / Block Parsing

**Location**: `crates/nif/src/blocks/properties.rs:238-254`
**Game Affected**: Oblivion (v10.1+)

### Description

The shader texture loop (lines 238-253) reads texture entries inline rather than calling `read_tex_desc()`. This inline path does not read `has_texture_transform` (bool) and conditional 32-byte transform data required for `version >= 10.1.0.0`. Meanwhile, the main `read_tex_desc()` function at line 280-304 correctly handles this for both new and old format paths.

**Root cause of the previously-noted NiTexturingProperty shortfall.** The shortfall is per-shader-texture, not a fixed 1 byte. For the common case of `num_shader_textures=0`, there is no shortfall.

### Suggested Fix

Refactor shader texture loop to call `read_tex_desc()` (or extract the transform-reading logic), ensuring `has_texture_transform` is consumed for each shader texture entry.

### Completeness Checks

- [ ] **SIBLING**: Verify `read_tex_desc()` covers all version paths correctly
- [ ] **TESTS**: Add test with shader textures that have texture transforms
- [ ] **UNSAFE**: N/A

🤖 Generated with [Claude Code](https://claude.com/claude-code)
