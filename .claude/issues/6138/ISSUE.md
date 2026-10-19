# ECS-SD-04: `Material::source_shader_type` is write-only

Labels: low,ecs,nifal,bug
URL: https://github.com/matiaszanolli/ByroRedux/issues/6138

**Severity**: LOW · **Dimension**: 8 NIFAL Canonical Material in the Component Layer

## Location
`crates/core/src/ecs/components/material.rs` (field `Material::source_shader_type`), `byroredux/src/material_translate.rs` (the only writer, `source_shader_type: source.shader_type`), `crates/save/src/snapshot.rs` (the v112 -> v113 bump note)

## Description
The field's doc says "downstream consumers that need the authored provenance read this", but `rg 'source_shader_type'` finds no reader outside tests (verified at publish time: writer in `material_translate.rs`, field and `Default` in `material.rs`, a pin test, the snapshot note). The commit that added it (`2266f7778`, Fix #4256) says the one candidate reader (the overlay's `TextureSlotContext`, cell loader) was deliberately not retargeted because it runs on the imported tier before a canonical `Material` exists. So the canonical tier now carries a 4 B field per `Material` row that nothing consumes, and because `Material` is a registered saved column the addition bumped `FORMAT_MAJOR` 112 to 113, invalidating every existing save.

Extension from the NIFAL lens (AUDIT_NIFAL_2026-10-19.md): the absence is structural, not an oversight. Every consumer in the area that needs the authored shader type runs before `translate_material`, on the imported tier (`select_facegen_diffuse(.., mesh.material.material_kind)`, the REFR overlay's `TextureSlotContext`, the FO3/FNV hair branch - see NIFAL-SD-03), so it cannot read a canonical field that does not exist yet. Downstream of the boundary the only consumer of the authored type is the shader, which reads `material_kind`. The field is not recorded in `nifal.md`'s ledger (parked-not-leak vs. reserved). Verified clean in the same commit: single-mutation-site rule holds, `Material::default()` gives 0, the pin `source_shader_type_survives_the_glass_reassignment_of_material_kind` passes.

## Impact
Forced save invalidation and per-entity memory for no behaviour change. If a consumer is planned (the issue's intent) this is moot; as shipped it is speculative canonical state.

## Related
#4256 (closed, `2266f7778`), NIFAL-SD-03 (needs the same discriminator at the boundary). Stale skill text: `.claude/commands/audit-nifal/SKILL.md` still lists "the shader-type discriminator never reaches the canonical `Material` (#4256)" as known-open - route to `/audit-sync`.

## Suggested Fix
Either land the first reader (retarget the overlay by threading the authored type through the imported tier it already reads) or note in the field doc and `nifal.md` that it is reserved with no reader yet (parked-not-leak, naming the first planned reader), so the next `FORMAT_MAJOR` bump can fold it.

Source: docs/audits/AUDIT_ECS_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the NIFAL parser->`Material` boundary - never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
