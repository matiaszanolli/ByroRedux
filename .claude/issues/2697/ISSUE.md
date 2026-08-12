# #2697: NIFAL-D8-2026-08-12-05: `supplemental_texture_indices` is a third hand-written role walk with no lockstep test

- **Severity**: LOW
- **Dimension**: Shader-flags/Effects (texture roles)
- **Tier Violated**: none today (verified correct) — regression surface only
- **Game Affected**: all
- **Location**: `byroredux/src/render/static_meshes.rs:561-574` vs `crates/renderer/src/vulkan/material.rs:415-430` and `crates/renderer/src/vulkan/context/mod.rs:492-504`
- **Status**: NEW
- **Description**: Beyond the two role walks the spec names (`map_ref`,
  compiler-protected; `values()`, not), there is a third: a positional `[u32; 12]`
  built in `byroredux` and indexed back out through `supplemental_texture_slot::*`
  constants in `byroredux_renderer`. Nothing couples the two orders. Verified
  correct today (tint, inner_layer, specular, lighting, flow, wrinkle,
  reflectance, emittance_gradient, decals 0-3), and the GPU side is protected by
  `material_hash_matches_gpu_material_field_hash` plus the `offset_of!` pins — but
  the CPU-side ordering has no test at all.
- **Evidence**: `grep -rn supplemental_texture_slot --include='*.rs' | grep -i test` → no hits.
- **Impact**: Inserting a constant mid-list silently shifts every following role by
  one — tint sampled as specular, etc. — with no compile error and no failing test.
- **Related**: the `values()` regression surface documented in `docs/engine/nifal.md`.
- **Suggested Fix**: Index the constants when building the array
  (`arr[slot::TINT] = …`), or add an explicit ordering test.

---

---
**Source**: `docs/audits/AUDIT_NIFAL_2026-08-12.md` (finding `NIFAL-D8-05`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs`, per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

