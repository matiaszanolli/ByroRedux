# #3589 — REN-2026-08-30-D6-02: `ParticleEmitter::effect_shader_flags` is the one authored emitter override written *outside* `apply_emitter_overlays`, duplicated byte-for-byte at both spawn sites

**Labels**: `low,renderer,nifal,tech-debt,bug`
**Filed**: 2026-08-30 via `/audit-publish`
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-30.md`

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is
> authoritative for current state — `gh issue view 3589 --json state`.

---

- **Severity**: LOW
- **Dimension**: NIFAL Material (particle slice)
- **Location**: `byroredux/src/systems/particle.rs` (`apply_emitter_overlays`), `byroredux/src/scene/nif_loader.rs:627-628`, `byroredux/src/cell_loader/spawn.rs:1074-1075`
- **Status**: OPEN — new (`effect_shader_flags` landed in `70f1bb74`/#2610, after the 2026-08-27 sweep)
- **Description**: `apply_emitter_overlays` is documented as "the **single overlay boundary** that folds every authored emitter override … onto a name-heuristic preset", explicitly so "a newly-wired authored field can no longer silently diverge the two load paths (#1513)". #2610 wired a new authored field — the `BSEffectShaderProperty` payload now carried on both `ImportedParticleEmitter` and `ImportedParticleEmitterFlat` as `effect_shader` — and wrote it into the preset with a hand-copied line at each spawn site instead of routing it through that helper. #3344's sibling `max_particles`, landing in the same delta, *did* go through the helper (it is the 9th parameter), so the two new fields took opposite routes.
- **Evidence**:
  - `apply_emitter_overlays`'s parameter list ends at `max_particles: Option<u32>`; the function body never mentions `effect_shader_flags` (grep: the only `effect_shader_flags` hits in `systems/particle.rs` are zero).
  - `scene/nif_loader.rs:628`: `preset.effect_shader_flags = crate::cell_loader::pack_effect_shader_flags(emitter.effect_shader.as_ref());`
  - `cell_loader/spawn.rs:1075`: `preset.effect_shader_flags = crate::cell_loader::pack_effect_shader_flags(em.effect_shader.as_ref());` — byte-identical modulo the binding name.
  - Each site's comment points at the other ("Mirrored in `cell_loader::spawn::spawn_particle_emitters`" / "see the sibling site in `scene/nif_loader.rs`") — hand-synced duplication, the shape `attach_blend_and_facing_markers` (#2490) was extracted to eliminate for the mesh slice.
  - The two new tests in `render/particles.rs` (`forwards_authored_effect_shader_flags`, `unauthored_effect_shader_flags_stay_zero`) set the field directly on the component; neither exercises either spawn site, so nothing fails if one of the two lines is deleted.
- **Impact**: No behavioural divergence today (the two lines agree). The regression surface is the one #1513 closed for the other four overlays: a future change to how the effect payload is packed — a gate, a merge with `pack_imported_material_flags`, a `None` guard — applied at one site renders the same NIF differently depending on whether it was loaded loose or placed as a REFR, with no test that can see it. Secondary: unlike every other overlay the assignment is unconditional rather than `if let Some(…)`, so it also overwrites rather than overlays (harmless only because all seven presets initialise the field to `0`).
- **Suggested Fix**: Add `effect_shader: Option<&BsEffectShaderData>` (or the already-packed `u32`) as a parameter of `apply_emitter_overlays`, pack inside it, and delete both hand-copied lines. Extend `apply_emitter_overlays_applies_color_rate_size_and_force_fields` and `apply_emitter_overlays_none_inputs_keep_preset_defaults` to cover it, matching how `max_particles` was handled in the same commit range.

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D6-02

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
