# SKY-2026-08-27b-D7-01: `material_translate.rs` still documents the retired 348-byte `GpuMaterial` — the one site #3240's sweep missed

- **Severity**: LOW
- **Dimension**: 7 (NIFAL canonical material translation)
- **Location**: `byroredux/src/material_translate.rs:77`
- **Confidence**: CONFIRMED

## Description

The `material_optical_scalar` doc justifies overloading `ior` by citing the record's size: *"without adding another field to the hot 348-byte GPU material record."* `GpuMaterial` has since grown 348 → 364 → 396 → 432 B; the live pin is `gpu_material_size_is_432_bytes` (`crates/renderer/src/vulkan/material.rs:46`, `:71`, `:87`). #3240 swept exactly this stale figure out of `crates/renderer/shaders/include/bindings.glsl`; this occurrence, in the NIFAL boundary that reasons about the record's cost, was not in that sweep.

## Evidence

`grep -rn "348" byroredux/src/material_translate.rs` → one hit, line 77. `crates/renderer/src/vulkan/material.rs:43` documents the growth chain itself (`… → 348 B (common supplemental texture roles) → 364 B (#2221 animated …)`).

## Impact

Documentation only, but it is a **GPU layout contract** number in the module whose whole job is deciding what fits in that record — the class of stale figure the audit-hygiene rule calls out by name.

## Suggested Fix

Change to 432 B, or better, drop the literal and cite `gpu_material_size_is_432_bytes` so it cannot drift again.

## Related

#3240 (the `bindings.glsl` sweep this site was missed by), #2222. Distinct from #3370, which is a different stale claim in the same file's Phase-2 module header.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.

---

*Filed from `docs/audits/AUDIT_SKYRIM_2026-08-27b.md` (`/audit-skyrim`).*
