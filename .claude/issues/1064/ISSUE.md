# #1064 — REN-D14-NEW-04: Dead overflow warn in upload_materials

**Severity**: LOW  
**Audit**: `docs/audits/AUDIT_RENDERER_2026-05-14_DIM14.md`  
**Location**: `crates/renderer/src/vulkan/scene_buffer/upload.rs:272-278`

## Summary

`if materials.len() > MAX_MATERIALS { log::warn!(…) }` is unreachable dead code.
`upload_materials` is only called with `material_table.materials()`, and `intern()` caps the Vec at `MAX_MATERIALS` before any entry is pushed.

## Fix

Delete lines 272-278 (the `if` block and its body). Retain the `.min(MAX_MATERIALS)` clamp on line 271.
