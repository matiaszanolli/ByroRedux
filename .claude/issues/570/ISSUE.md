# #570 — SK-D3-03: MaterialInfo::material_kind truncated to u8

**Severity:** LOW (safe today, fragile on extension)
**Labels:** bug, low, nif-parser, safety
**Source:** AUDIT_SKYRIM_2026-04-22.md
**GitHub:** https://github.com/matiaszanolli/ByroRedux/issues/570

## Location
- `crates/nif/src/import/material.rs:239, 586`

## One-line
Parser `shader_type: u32`, GPU struct `material_kind: u32`, importer narrows to `u8` and widens back. `as u8` silently masks values ≥ 256. Safe for 0-20 + 100 today.

## Fix sketch
Widen `MaterialInfo::material_kind` to `u32`; drop the `as u8` cast. Two-line change.

## Next
`/fix-issue 570`
