# #3591 — REN-2026-08-30-D6-04: `Material::parallax_height_in_alpha` was added to the canonical struct without extending the canonical-completeness harness

**Labels**: `low,renderer,nifal,test-gap,bug`
**Filed**: 2026-08-30 via `/audit-publish`
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-30.md`

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is
> authoritative for current state — `gh issue view 3591 --json state`.

---

- **Severity**: LOW
- **Dimension**: NIFAL Material
- **Location**: `byroredux/src/material_translate.rs` (`canonical_completeness_harness::kitchen_sink_source`, `translate_material_copies_every_canonical_field`)
- **Status**: OPEN — new field, sibling of (not covered by) #3462
- **Description**: The harness's stated contract is that "deliberately reverting any single `source.X` → `material.X` line in `translate_material` fails exactly the corresponding assertion below". #3530 added a 60th `Material` field and its copy line (`parallax_height_in_alpha: source.parallax_height_in_alpha`) without adding it to `kitchen_sink_source()` (where it stays at the `ImportedMaterial::default()` value `false`) or asserting it. Deleting the copy line leaves the harness green.

  This is distinct from #3462: that issue enumerates four fields already uncovered at the 2026-08-27 sweep (`water_shader_flags` / `is_water_shader` at the NIFAL↔WATAL seam, plus two more). `parallax_height_in_alpha` did not exist then. The point is not the field count — it is that a new field shipped through the boundary without the harness being extended in the same commit, which is the failure mode #3462 was filed to stop recurring.
- **Evidence**:
  - Script-checked at HEAD: `Material` declares 60 `pub` fields; all 60 are written in the `translate_material` literal (`material_path` via destructuring shorthand). Ten are absent from the harness assertions; two of those ten (`shader_type_fields`, `effect_falloff`) are covered by multi-line assertions. The remaining eight are `water_shader_flags`, `is_water_shader`, `grayscale_to_palette_scale`, `ior`, `sheen`, `sheen_tint`, `anisotropic`, `parallax_height_in_alpha`. The first four are #3462's; `sheen`/`sheen_tint`/`anisotropic` are deliberate `0.0` literals with no source field (#2514); `parallax_height_in_alpha` is the new gap.
  - `kitchen_sink_source()` sets `texture_clamp_mode: 1`, `src_blend_mode: 2`, `dst_blend_mode: 3` "so the round-trip assertion below actually exercises the copy" but no `parallax_height_in_alpha: true`.
  - Round-tripping *elsewhere* is fine: `crates/nif/src/import/tests/material_texture.rs:282,303` pins the importer-side set/clear, and `byroredux/src/save_io/serde_default_guard_tests.rs:337` pins the `FORMAT_MAJOR` 10 save shape. Only the translate boundary itself is unpinned.
- **Impact**: Test-gap only; the copy is present and correct at HEAD. A future refactor of the `Material` literal that drops the line silently reverts every Oblivion `APPLY_HILIGHT2` mesh to `.r`-channel parallax against a normal map — i.e. sampling the packed normal's red channel as height — with the full workspace suite green.
- **Suggested Fix**: Add `parallax_height_in_alpha: true` to `kitchen_sink_source()` and `assert!(material.parallax_height_in_alpha)` to `translate_material_copies_every_canonical_field`, next to the `#2571` clamp/blend block. Fold the same edit into #3462's fix so the harness closes on all five at once, and consider adding a field-count pin (the same `include_str!`-scan trick `documented_texture_role_list_matches_the_struct` already uses in this file) so the next added field fails the harness rather than slipping past it.

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D6-04

## Dedup cross-reference

Sibling of **#3462**, which enumerates four *other* uncovered fields
(`water_shader_flags`, `is_water_shader`, `ior`, `effect_shader_flags`).
`parallax_height_in_alpha` did not exist when #3462 was filed. Fold this into #3462's fix
so the harness closes on all five at once.


## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
