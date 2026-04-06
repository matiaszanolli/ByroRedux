# NIF-404: BsTriShape duplicates ~130 lines of material extraction

## Finding

**Audit**: NIF 2026-04-05b | **Severity**: LOW (Structural) | **Dimension**: Import Pipeline

**Location**: `crates/nif/src/import/mesh.rs:126-258`
**Game Affected**: Skyrim+

`extract_bs_tri_shape()` re-implements material property extraction inline instead of delegating to `extract_material_info()`. This creates parity drift — NIF-403 (missing BSEffectShaderProperty two_sided check) is a concrete example.

Refactor to use shared MaterialInfo extraction parameterized by shader/alpha property refs.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
