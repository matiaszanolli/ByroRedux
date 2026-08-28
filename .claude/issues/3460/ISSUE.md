# Issue #3460: NIFAL-2026-08-27-03: Material::{soft_lighting, rim_lighting, back_lighting} are write-only — the same fact already rides effect_shader_flags, and only the packed word has a consumer

**Labels**: low, nifal, nif-parser, tech-debt, bug
**Filed**: 2026-08-27 via /audit-publish

**Severity**: LOW
**Dimension**: Material (Dim 1)
**Tier Violated**: `no-leak` (two canonical representations of one fact, one of them unread)
**Game Affected**: Skyrim (NIF gates), FO4+ (BGSM gates)
**Location**: `crates/core/src/ecs/components/material.rs:291-297` (the fields), `byroredux/src/material_translate.rs:523-525` (the only writer), `byroredux/src/cell_loader.rs:267-275` (`pack_imported_material_flags`, which reads `ImportedMaterial`, not these)
**Source**: `docs/audits/AUDIT_NIFAL_2026-08-27.md` — NIFAL-2026-08-27-03

## Description

`translate_material` copies the three authored gates onto the canonical
`Material` *and* — in the same call — packs `MAT_FLAG_SOFT_LIGHTING` / `_RIM_` /
`_BACK_` into `effect_shader_flags` via `pack_imported_material_flags(source)`,
which takes `&ImportedMaterial` and therefore derives the bits from the raw tier,
not from the canonical bools it just wrote. The shader reads only the packed word
(`include/lighting.glsl:96`, `:111`, `:119`). A full-tree grep for
`.soft_lighting` / `.rim_lighting` / `.back_lighting` outside the BGSM crate, the
NIF importer and tests finds no reader of the `Material` fields at all.

## Evidence

The three writer lines in `material_translate.rs:523-525` and the three
`if material.<x>_lighting` packer lines in `cell_loader.rs:267-275` — the latter's
`material` binding is the `&ImportedMaterial` parameter, one tier below.
`Material` is a save/restore unit (`crates/save/src/driver.rs::restore_world`), so
both representations round-trip independently and nothing reconciles them.

## Impact

None today; both are written from the same source in the same call. It is a
latent divergence surface (any future `mat.set`-style editor, or a restored save,
can move one without the other) and a small maintenance cost — a reader who finds
the canonical bool reasonably assumes it is what the renderer consults.

Note this is *not* the `cell_loader/spawn/mesh_instance.rs:193-195`
`TextureSlotContext` re-read of the raw tier: that runs in `resolve_mesh_paths`,
**before** `translate_material`, so it structurally cannot read the canonical
component.

## Related

- `#2571` / OBL-D5-01 (the same "spawn sites should read the canonical component"
  argument, applied three lines away for `texture_clamp_mode` / `src_blend_mode` /
  `dst_blend_mode`)
- NIFAL-2026-08-27-01 (this audit's HIGH touches the same call — worth fixing
  together)

## Suggested Fix

Either derive the three flag bits from the canonical bools after the literal is
built (one representation feeds the other), or drop the bools and keep
`effect_shader_flags` as the single canonical carrier.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other `Material` fields duplicated between a canonical bool and a packed flag bit)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
