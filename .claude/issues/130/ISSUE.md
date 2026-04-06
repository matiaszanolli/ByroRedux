# NIF-405: BSShaderPPLightingProperty normal map not extracted (FO3/FNV)

## Finding

**Audit**: NIF 2026-04-05b | **Severity**: LOW | **Dimension**: Import Pipeline

**Location**: `crates/nif/src/import/material.rs:207-227`
**Game Affected**: FO3/FNV

Extracts diffuse texture from `textures[0]` but not normal map from `textures[1]`. BSShaderTextureSet uses the same slot layout as Skyrim (0=diffuse, 1=normal). FO3/FNV meshes with normal maps render flat-lit.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
