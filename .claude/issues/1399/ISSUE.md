## MEM-02: MeshRegistry handle allocation uses len() as u32 with no overflow guard (TextureRegistry has the guard)

**Severity:** MEDIUM | **Audit:** [AUDIT_SAFETY_2026-06-01](docs/audits/AUDIT_SAFETY_2026-06-01.md) | **Dimension:** memory
**File:** `crates/renderer/src/mesh.rs:284`

## Recommended Fix

Add explicit capacity check before the cast: if self.meshes.len() >= u32::MAX as usize { return Err(...); }. Add a max_meshes field mirroring TextureRegistry.max_textures.

## Completeness Checks

- [ ] **TESTS**: Regression test or reference added for this specific fix

---
*Filed by audit-publish from AUDIT_SAFETY_2026-06-01.md — findings verified against live code during audit.*