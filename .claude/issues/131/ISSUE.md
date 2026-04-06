# NIF-406: NiTexturingProperty bump/normal texture not extracted (Oblivion)

## Finding

**Audit**: NIF 2026-04-05b | **Severity**: LOW | **Dimension**: Import Pipeline

**Location**: `crates/nif/src/import/material.rs:193-205`
**Game Affected**: Oblivion

Extracts only `base_texture` from NiTexturingProperty; ignores `bump_texture` (slot 5) and `normal_texture` (slot 6). The struct has these fields parsed. For Oblivion, the bump map slot was used for tangent-space normal maps.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
