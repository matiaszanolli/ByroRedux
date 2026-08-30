# #3567 — REN-2026-08-30-D6-01: the Oblivion `APPLY_HILIGHT2` normal-map alpha is consumed as BOTH parallax height and the normal-alpha-as-spec mask — the render-side predicate never consults `Material::parallax_height_in_alpha`

**Labels**: `medium,renderer,nifal,game:oblivion,bug`
**Filed**: 2026-08-30 via `/audit-publish`
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-30.md`

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is
> authoritative for current state — `gh issue view 3567 --json state`.

---

- **Severity**: MEDIUM
- **Dimension**: NIFAL Material
- **Location**: `byroredux/src/material_translate.rs` (`normal_alpha_spec_binding_applies`, `normal_alpha_spec_applies`), `byroredux/src/render/static_meshes.rs` (`build_static_mesh_draws`, lines ~306-312 and ~474-484), `crates/nif/src/import/material/legacy_properties.rs` (the `APPLY_HILIGHT2` arm)
- **Status**: OPEN — new (the `parallax_height_in_alpha` field landed in `19813460`, after the 2026-08-27 sweep)
- **Description**: #3530 resolved a per-game channel-meaning decision at the NIFAL boundary: `Material::parallax_height_in_alpha` records that this material's height values live in the bound texture's **alpha**, because Oblivion ships no `_p.dds` and `legacy_properties.rs` therefore binds the *normal* map into `MaterialTextureSet::height`. The render path transports that as `PARALLAX_ALPHA_HEIGHT_BIT` on `parallax_map_index`.

  Fifty lines further down in the same loop, `normal_alpha_spec_binding_applies` makes an *independent* claim about the same channel of the same texture — that the normal map's alpha is a per-pixel **specular-intensity mask** — and re-points the gloss slot at the normal map with `NORMAL_ALPHA_SPEC_BIT`. It reads `material_kind`, `normal_has_alpha`, `normal_map_index` and `gloss_map_index`; it does **not** read `parallax_height_in_alpha`. The two are not mutually excluded anywhere.

  For an `APPLY_HILIGHT2` mesh the preconditions of the second predicate are satisfied by construction: `normal_has_alpha` must be true (that alpha *is* the height payload), `normal_map_index != 0` (the parallax slot was bound from it), and `material_kind < 100` for ordinary Oblivion architecture. Only a bound `NiTexturingProperty.gloss_texture` (`gloss_map_index != 0`) suppresses it.
- **Evidence**:
  - `legacy_properties.rs`: `if tex_prop.apply_mode == APPLY_HILIGHT2 && info.parallax_map.is_none() { … info.parallax_map = Some(normal); info.parallax_height_in_alpha = true; }`
  - `crates/nif/src/import/material/mod.rs:1249` — `height: self.parallax_map`, so `textures.height` and `textures.normal` resolve to the *same* path and therefore the same bindless handle.
  - `static_meshes.rs`: `if parallax_map_index != 0 && mat.is_some_and(|m| m.parallax_height_in_alpha) { parallax_map_index |= PARALLAX_ALPHA_HEIGHT_BIT; }`
  - `static_meshes.rs`: `if normal_alpha_spec_binding_applies(mat, normal_has_alpha, material_kind, metalness, normal_map_index, gloss_map_index) { gloss_map_index = normal_map_index | NORMAL_ALPHA_SPEC_BIT; }` — no `parallax_height_in_alpha` term.
  - `normal_alpha_spec_applies` body is exactly `material_kind < 100 && normal_map_index != 0 && gloss_map_index == 0`.
  - Both consumers then read the same texel: `material_sampling.glsl::sampleParallaxHeight` returns `texel.a`, and `triangle.frag:1247-1255` does `normalAlphaSpecMask = glossTexel.a; specStrength *= normalAlphaSpecMask;`.
  - `normal_has_alpha` originates from `dds::format_has_alpha` on the bound normal (`scene/nif_loader.rs:1101`), so it is true precisely for the population that carries height data.
- **Impact**: On the `APPLY_HILIGHT2` population (the commit message cites 1,433 properties across 741 vanilla Oblivion meshes) the specular strength is multiplied by the **height field**: crevices go matte and raised brickwork goes glossy, with the modulation tracking displacement rather than any authored spec mask. Symmetrically, the engine now asserts two mutually exclusive meanings for one channel in one draw with nothing arbitrating — which is the exact class of render-time channel-meaning re-derivation NIFAL exists to eliminate, reintroduced one predicate away from the field that was added to prevent it. Confined to Oblivion; every other producer leaves `parallax_height_in_alpha` false and is unaffected.
- **Suggested Fix**: Make the two exclusive at the canonical boundary rather than in the draw loop. Thread `parallax_height_in_alpha` into `normal_alpha_spec_applies` (or add it to `normal_alpha_spec_binding_applies`'s inputs, which already takes `Option<&Material>`) and return `false` when it is set, so a material whose normal alpha was already claimed as height cannot also claim it as a spec mask. Pin it with a test alongside the existing `normal_alpha_spec_binding_applies` cases in `material_translate.rs:1743-1770`. Before landing, census `NiTexturingProperty.gloss_texture` fill on the `APPLY_HILIGHT2` meshes to confirm the suppressing `gloss_map_index != 0` arm is as rare as it appears (the fix is correct either way; the census only sizes the affected population).

---
- **Cross-dimension corroboration**: Found independently three times — also filed as *D2-02* (SSBO/indexing) and *D19-02* (tangent-space). All three traced the same two predicates and reached the same conclusion; the write-up below is the NIFAL-dimension one, which carries the corpus figure.

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D6-01

## Population census (added at publish time)

A sibling audit measured `Material::parallax_height_in_alpha` as true on **0 of 35,322**
vanilla Oblivion meshes (0 of 1,430 `APPLY_HILIGHT2` properties carry a normal/bump slot),
so the channel collision has **no live population on shipped Oblivion content today**.
File/fix it as an arbitration-correctness change; there is no visual repro to chase.

Related: the missing alpha-presence gate on the same `#3530` route is filed separately
(see the `D19-01` issue from this same report).


## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
