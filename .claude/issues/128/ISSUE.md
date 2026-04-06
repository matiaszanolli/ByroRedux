# NIF-403: BsTriShape two_sided check misses BSEffectShaderProperty

## Finding

**Audit**: NIF 2026-04-05b | **Severity**: LOW | **Dimension**: Import Pipeline

**Location**: `crates/nif/src/import/mesh.rs:172-179`
**Game Affected**: Skyrim+

Only checks BSLightingShaderProperty for double-sided flag (`shader_flags_2 & 0x10`); misses BSEffectShaderProperty which uses the same flag bit for glow/effect meshes.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
