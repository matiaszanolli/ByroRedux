# Issue #3462: NIFAL-2026-08-27-04: the canonical-completeness harness's "reverting any single source.X line fails an assertion" contract is false for four fields, two of which gate the NIFAL↔WATAL seam

**Labels**: low, nifal, test-gap, bug
**Filed**: 2026-08-27 via /audit-publish

**Severity**: LOW (test-coverage gap; the code is correct today)
**Dimension**: Completeness signal (Dim 9)
**Tier Violated**: `parked-not-leak` verification gap
**Game Affected**: all
**Location**: `byroredux/src/material_translate.rs:1904-2010` (`translate_material_copies_every_canonical_field`), against the copies at `:441-442`, `:504-508`, `:534`, `:545`
**Source**: `docs/audits/AUDIT_NIFAL_2026-08-27.md` — NIFAL-2026-08-27-04

## Description

The harness's doc-comment states *"Deliberately reverting any single `source.X` →
`material.X` line in `translate_material` fails exactly the corresponding
assertion below — this is the 'fails on a deliberately reintroduced boundary
drop' contract #2214 asked for."* Four copies have no corresponding assertion:
`water_shader_flags` (`:441`), `is_water_shader` (`:442`), `ior` ←
`material_optical_scalar(material_kind, refraction_strength)` (`:545`), and
`effect_shader_flags` (`:504-508`). `grayscale_to_palette_scale` (`:534`) is
uncovered by the harness but *is* covered by a dedicated sibling test
(`translate_material_copies_grayscale_to_palette_scale`, `:1497-1507`), so it is
not part of this gap.

`is_water_shader` matters most: it is the sole gate both spawn sites read to
decide whether to call `attach_mesh_water` (`scene/nif_loader.rs:925`,
`cell_loader/spawn/mesh_instance.rs:825`), i.e. whether a dedicated
`WaterShaderProperty` mesh crosses the NIFAL↔WATAL seam at all. Silently changing
that copy to `false` removes every mesh-authored water plane in every game and the
whole suite stays green.

## Evidence

The `kitchen_sink_source()` fixture sets neither `water_shader_flags` nor
`is_water_shader` (both fall to `..ImportedMaterial::default()`), and the
assertion block at `:1907-1949` contains no `material.water_shader_flags` /
`material.is_water_shader` / `material.ior` / `material.effect_shader_flags`
line — 57 asserts in the harness body, zero matching those four fields. The
`Material` literal in `translate_material` has no `..Default::default()` tail, so
deleting a line is a compile error — the reachable regression is a line *changed*
to a constant, which is exactly what the harness claims to catch.

## Impact

None today. The harness is the designated whole-boundary guard (`#2214` was filed
because `crates/nif`'s raw-tier harness physically cannot reach
`translate_material`), so a false completeness claim on it is the same defect
shape `#3438` (SAFE-2026-08-27b-03) raises for `sanitize_finite`'s hand-typed
field list.

## Related

- `#2214` / NIFAL-D9-02 (the harness this contract belongs to)
- `#2532` / NIFAL-D9-04 (the *breadth* gap — other translate boundaries have no
  harness at all; this finding is the *depth* gap inside the Material one)
- `#3438` / SAFE-2026-08-27b-03 (the same hand-transcribed-list class)
- `docs/engine/watal.md`

## Suggested Fix

Add the four assertions with distinctive fixture values (`water_shader_flags:
0x5A`, `is_water_shader: true` — noting the fixture must then not exercise the
water path, or split into a second fixture; `refraction_strength` +
`material_kind = MATERIAL_KIND_FIRE_REFRACTION` for `ior`; a non-zero
`effect_shader` for the packed word). Better still, follow the
`shader_constants.rs` / `skinned_blas_refit.rs` precedent already used elsewhere
in this repo and add an `include_str!("material_translate.rs")` scan asserting
every `<field>: source.<field>` line in the `Material` literal has a matching
`assert` in the harness.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other `translate_*` completeness harnesses and their claimed contracts)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
