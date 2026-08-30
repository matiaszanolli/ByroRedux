# #3590 — REN-2026-08-30-D6-03: the particle boundary drops `MaterialInfo.greyscale_lut_map`, so the two palette bits #2610 now forwards are structurally inert

**Labels**: `low,renderer,nifal,bug`
**Filed**: 2026-08-30 via `/audit-publish`
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-30.md`

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is
> authoritative for current state — `gh issue view 3590 --json state`.

---

- **Severity**: LOW
- **Dimension**: NIFAL Material (particle slice)
- **Location**: `crates/nif/src/import/walk/mod.rs` (`extract_particle_material`, `ParticleMaterial`), `byroredux/src/render/particles.rs` (`emit_particles`), `crates/core/src/ecs/components/particle.rs` (`ParticleEmitter`)
- **Status**: OPEN — new (introduced by the #2610 wiring in `70f1bb74`)
- **Description**: `extract_particle_material` builds a full `MaterialInfo` through `extract_material_info_from_refs` and now harvests four things from it — `texture_path`, `src_blend`, `dst_blend`, `effect_shader`. The `effect_shader` payload carries `effect_palette_color` / `effect_palette_alpha`, which `pack_effect_shader_flags` turns into `MAT_FLAG_EFFECT_PALETTE_COLOR` / `MAT_FLAG_EFFECT_PALETTE_ALPHA` on `ParticleEmitter::effect_shader_flags`. But the LUT *texture* those two bits index — available on the very same `MaterialInfo` as `greyscale_lut_map`, and resolved into `MaterialTextureSet::greyscale_lut` for the mesh path — is dropped at this function: `ParticleMaterial` has no field for it, `ImportedParticleEmitter{,Flat}` has no field for it, `ParticleEmitter` has no slot for it, and `emit_particles` hardcodes `greyscale_lut_index: 0`.
- **Evidence**:
  - `struct ParticleMaterial { texture_path, src_blend, dst_blend, effect_shader }` — no LUT role.
  - `crates/nif/src/import/material/mod.rs:468` `pub greyscale_lut_map: Option<FixedString>` and `:1270` `greyscale_lut: self.greyscale_lut_map.or_else(…)` — the role exists and the mesh path consumes it.
  - `render/particles.rs`: `greyscale_lut_index: 0` with the comment "particles never carry the greyscale palette LUT either; the bindless 0 slot signals 'no LUT'".
  - `triangle.frag:862-864` and `:1151-1152` both gate the palette remap on `mat.greyscaleLutIndex != 0u`, so the forwarded bits can never fire on a particle draw.
  - The adjacent `render/particles.rs` comment states this explicitly: "The palette bits stay inert while `greyscale_lut_index == 0`."
- **Impact**: No corruption — the gating is correct, and a bare palette bit on index 0 does not sample texture 0. The gap is a canonical-completeness one: a BGEM/`BSEffectShaderProperty` particle system that authored a greyscale→palette remap (the standard authoring for tinted smoke / energy FX) reaches the GPU with the *instruction* to remap and without the *palette*, so it renders as the un-remapped luminance sprite. Half of #2610's forwarded word is dead by construction, and the comment documenting that is easily read as "particles cannot author a LUT" rather than "we drop it one line above".
- **Suggested Fix**: Carry `greyscale_lut_map` on `ParticleMaterial` → `ImportedParticleEmitter{,Flat}` → a `greyscale_lut` path on `ParticleEmitter`, resolve it with the same `resolve_texture` call both spawn sites already make for the sprite, and forward the handle in `emit_particles` instead of the literal `0`. If the population turns out to be empty on installed corpora, census it and replace the "particles never carry" comment with the measured rate — the current wording asserts a format property that is not true.

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D6-03

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
