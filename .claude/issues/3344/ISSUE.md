# FNV-2026-08-26-D6-05

**Issue**: #3344
**Filed**: 2026-08-26 from `docs/audits/AUDIT_FNV_2026-08-26.md`

---

**Severity**: LOW
**Dimension**: 6 — Animation, Skinning & Particles
**Status**: NEW
**Source**: `docs/audits/AUDIT_FNV_2026-08-26.md` (audit HEAD `d6e16c90`)


**File**: `crates/nif/src/blocks/tri_shape/ni_tri_shape.rs:320-323`

**Premise verified**: the parser reads the field and *documents* what it is —
`// For NiPSysData on BS202, num_vertices_raw is BS Max Vertices — an upper
bound on runtime particle count, not a serialized array length.` — then sets
`array_count = 0` and never returns the value. `ImportedParticleEmitter`
carries no max-particle field, so `apply_emitter_overlays` leaves
`preset.max_particles` at the heuristic value (96 / 128 / 192 / 256 —
`core/src/ecs/components/particle.rs:290-489`) and the spawn loop hard-caps at
`let cap = em.max_particles as usize;` (`systems/particle.rs:413`). nif.xml
`NiParticlesData` (line 3993) documents the field as *"the maximum number of
particles (matches the number of vertices)"*.

**Evidence**:
```
$ ... --example _tmp_fnv_d6_budget -- "Fallout - Meshes.bsa"
emitters with authored rate+life: 205
  steady-state (rate x life) > 96  : 62 (30%)
  steady-state > 192 : 47 (23%)
  steady-state > 256 : 43 (21%)
  median: 25.0
    10800  meshes\effects\explosionsplash.nif
    10800  meshes\effects\watersurfaceexplosion01.nif
     9000  meshes\dlcanch\effects\dlcanchimpctexpsnowlg.nif
     6750  meshes\effects\impactexplosiondirtlarge.nif
```

**Impact**: 70% of FNV authored emitters fit under the cap (median 25), so this
is bounded — but the 21% that overrun are the high-visibility impact/explosion
splashes, which render ~2% of their authored particle count and then stall.
Because the authored rate now *does* reach the preset (D6-03's 100 files), the
overlay makes the truncation worse than it was with preset rates, silently.

**Fix sketch**: this is a deliberate perf budget, so the fix is not "raise the
cap to 10 800". Plumb `BS Max Vertices` onto `ImportedParticleEmitter`, clamp
`max_particles` to `min(authored, engine_ceiling)` in `apply_emitter_params`,
and log once at debug when the authored budget is clamped so the truncation is
visible instead of silent.

---

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
