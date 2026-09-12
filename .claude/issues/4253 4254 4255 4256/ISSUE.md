# #4253 — SKY-2026-09-11-D6-01: `.bto` object-LOD spawner leaks a GPU texture handle + descriptor slot per non-atlas sub-mesh texture on every unload

**Severity**: HIGH
**Dimension**: 6 — Specialty Blocks + Real-Data Rendering
**Location**: `byroredux/src/cell_loader/object_lod.rs:65-79,348-431,473-481,490-505`
**Status**: NEW

**Description**: `.bto` object-LOD sub-mesh textures are acquired (`resolve_texture`, refcounted) but never released. `ObjectLodBlock` has exactly one texture field (`texture_handle`, the shared worldspace atlas), documented as the only handle dropped on unload, while #3412 added per-sub-mesh texture resolution for the ~34% of vanilla bindings that name a distinct (non-atlas) texture — none of those handles are ever released, in both `unload_object_lod_block` and the `entities.is_empty()` early return. This is the same leak shape #1537 and #2758 already closed on the sibling terrain-LOD and early-return paths; #3412 reopened it on the object-LOD path.

**Evidence**: Confirmed in current code — `ObjectLodBlock` (`object_lod.rs:69-78`) declares only `texture_handle: u32` (comment: "Dropped once on unload"). The per-sub-mesh loop (`object_lod.rs:348-431`) resolves potentially many distinct textures into a local `resolved: FxHashMap<String, u32>` (line 353), but neither `unload_object_lod_block` (`object_lod.rs:491-505`, releases only `block.texture_handle`) nor the `entities.is_empty()` early return (`object_lod.rs:473-481`, releases only `atlas`) touches any handle from `resolved` beyond the atlas entry.

**Impact**: Accrues on every quad load across the level-4/8/16 object-LOD ring during exterior traversal; never reclaimed; pins VRAM + bindless descriptor slots against `TextureRegistry` LRU eviction. Unbounded over a session's exterior traversal.

**Related**: #1537, #2758 (closed the same leak shape on sibling paths); #3412 (introduced the per-sub-mesh resolution that reopened it here).

**Suggested Fix**: Track every distinct non-atlas texture handle resolved per quad (e.g. store the `resolved` map's values, deduped, alongside `texture_handle` in `ObjectLodBlock`) and release all of them in both `unload_object_lod_block` and the `entities.is_empty()` early return, mirroring the `.btr` distant-terrain LOD path's correct dual-release (per the report's Dimension 6 checklist item 4).

## Completeness Checks
- [ ] **SIBLING**: Confirm the `.btr` distant-terrain LOD path's texture release pattern (which the report notes already does this correctly) is followed exactly
- [ ] **TESTS**: A regression test loads and unloads an object-LOD quad with ≥2 distinct non-atlas sub-mesh textures and asserts the registry refcount returns to its pre-load value

---

# #4254 — SKY-2026-09-11-D6-02: ROADMAP.md prose Parser-coverage summary contradicts its own compatibility matrix on file counts and per-game percentages

**Severity**: LOW
**Dimension**: 6 — Specialty Blocks + Real-Data Rendering
**Location**: `ROADMAP.md:519` (prose "Parser coverage" summary) vs `ROADMAP.md:687,1514` (compatibility matrix)
**Status**: NEW

**Description**: `ROADMAP.md`'s prose "Parser coverage" summary contradicts its own compatibility matrix on three numbers: 184,886 (prose) vs. the matrix's cited totals (the matrix explicitly names 184,886 as superseded by #3369/#3466), and stale 100%/FO76-clean/Starfield-99.99% claims vs. the matrix's current FO76-98.18%-with-truncation-tail and Starfield-99.98%.

**Evidence**: Confirmed in current code — `ROADMAP.md:519-523` states "NIF parses across seven games (184 886 files on the latest sweep...)" and "Starfield at 99.99% aggregate", while `ROADMAP.md:687` cites Starfield at **99.98%** aggregate (120,524/120,543) and `ROADMAP.md:1514` cites **FO76 98.18%** with a named truncation tail (#3466) — directly contradicting the prose summary's implied FO76-clean claim.

**Impact**: Documentation-only, but it's precisely the premise a future audit would cite, risking a stale figure propagating into a new report.

**Suggested Fix**: Update the prose "Parser coverage" summary (line ~519) to match the compatibility matrix's current, sourced figures (Starfield 99.98%, FO76 98.18% with truncation tail), and cite the matrix as the single source of truth rather than restating numbers that can drift out of sync.

## Completeness Checks
- [ ] **TESTS**: N/A (documentation-only fix)

---

# #4255 — SKY-D7-2026-09-11-01: classify_glass_into_material silently overwrites authored Skyrim BSLightingShaderProperty shader types with glass

**Severity**: HIGH
**Dimension**: 7 — NIFAL Canonical Material Translation (Skyrim slice)
**Location**: `byroredux/src/helpers.rs:99-135`, called from `byroredux/src/material_translate.rs:649`
**Status**: NEW

**Description**: `Material.material_kind` is an overloaded union: low range `0..=20` is the verbatim authored Skyrim `BSLightingShaderProperty.shader_type`; `>= 100` is engine-synthesized. `classify_glass_into_material` protects only the synthesized range (`material_kind >= 100`) — every authored low-range shader type falls straight through to a bare keyword+coverage+metalness heuristic promotion to `MATERIAL_KIND_GLASS`, with no way back. The function's own doc comment (`helpers.rs:59-61`) asserts a keyword alone cannot override an authored shader type — true only for the effect-shader carrier, not the lit carrier this bug affects.

**Evidence**: Confirmed in current code — `helpers.rs:108-113`: `if material.material_kind >= 100 && material.material_kind != MATERIAL_KIND_GLASS && !effect_glass_carrier { return; }` only early-returns for the synthesized range; a low-range authored value (0-20) falls through to the `is_mirror_pane` check (which unconditionally zeroes `material_kind`) and the keyword-match glass promotion below it. Confirmed reachable by two real Skyrim populations: `MultiLayerParallax` (11) on layered ice surfaces (`icefrozen01`/`icecavewall01`/`icelakesurface`, widened into keyword reachability by #3359) loses its inner-layer-parallax dispatch in `triangle.frag` and takes flat glass refraction instead; and glowing soul gems (`gem` keyword) lose their glow dispatch to flat glass — the exact bug shape #2710 already fixed for the effect-shader carrier but not for the lit carrier. The `is_mirror_pane` arm additionally hard-zeroes `material_kind` unconditionally, destroying an authored `EnvironmentMap` (1) dispatch.

**Impact**: Ice surfaces and soul gems (real, named vanilla Skyrim assets) render as flat glass instead of their authored parallax/glow shader dispatch. Mirror surfaces lose their `EnvironmentMap` dispatch unconditionally.

**Related**: #2710 (fixed the same bug shape for the effect-shader carrier only); #4256 (structural root cause, filed as a companion MEDIUM finding).

**Suggested Fix**: Gate `classify_glass_into_material`'s override on the same external-material provenance check the effect-shader carrier already requires, so an authored low-range lit shader type is protected the same way a synthesized one is. Content-side reachability sizing (a BSA-wide BSLSP scan or `--bench-hold`/`byro-dbg` census) would quantify the full blast radius.

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: Fix stays at the NIFAL parser→`Material` boundary (`classify_glass_into_material`/`translate_material`), not pushed into the renderer — see `/audit-nifal`.
- [ ] **SIBLING**: Verify `is_mirror_pane`'s unconditional zero of `material_kind` is fixed alongside the keyword-glass-promotion path
- [ ] **TESTS**: Regression tests for `icefrozen01`/`icecavewall01`/`icelakesurface` (MultiLayerParallax) and a `gem`-keyword soul gem (glow) assert `material_kind` survives glass classification

---

# #4256 — SKY-D7-2026-09-11-02: ImportedMaterial.shader_type discriminator never crosses the NIFAL boundary into canonical Material

**Severity**: MEDIUM
**Dimension**: 7 — NIFAL Canonical Material Translation (Skyrim slice)
**Location**: `crates/nif/src/import/types.rs:760`, `byroredux/src/material_translate.rs:486-647`
**Status**: NEW

**Description**: Structural root cause of the companion HIGH finding (#4255): `ImportedMaterial.shader_type` never crosses the NIFAL boundary into `Material`. Only the per-variant *payload* (`shader_type_fields`) crosses; the discriminator that identifies which variant it belongs to does not, so once `material_kind` (seeded from the same raw value) is reassigned by the glass classifier, no canonical field retains the original authored provenance — forcing at least one downstream consumer (`TextureSlotContext` at cell-spawn) to read back into the raw `ImportedMaterial` tier directly, a NIFAL single-boundary violation in the making.

**Evidence**: Confirmed in current code — `ImportedMaterial.shader_type: u32` is defined at `crates/nif/src/import/types.rs:760`, but the canonical `Material` struct (`crates/core/src/ecs/components/material.rs`) has no standalone `shader_type` field — only `shader_type_fields: Option<Box<ShaderTypeFields>>` (the payload, not the discriminator) crosses via `translate_material` (`material_translate.rs:486-647`).

**Impact**: No independent runtime effect beyond enabling the companion HIGH finding — this is the structural gap that makes that overwrite unrecoverable once it happens. Also forces at least one downstream consumer to bypass the canonical boundary and read the raw `ImportedMaterial` tier directly.

**Related**: #4255 (the HIGH finding this enables); shared suggested-fix direction.

**Suggested Fix**: Add an explicit canonical `source_shader_type` field on `Material`, distinct from the engine-dispatch `material_kind`, so downstream consumers (including the glass classifier and `TextureSlotContext`) can recover the authored provenance without reading back into `ImportedMaterial`.

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: The new field is populated only at the single `translate_material` boundary, never re-derived at render time — see `/audit-nifal`.
- [ ] **SIBLING**: Audit `TextureSlotContext`'s existing read-back into `ImportedMaterial` and retarget it at the new canonical field once added
- [ ] **TESTS**: A regression test asserts `Material::source_shader_type` survives `classify_glass_into_material` unchanged even when `material_kind` is reassigned

**Disposition (this batch, 2026-09-12)**: Left OPEN, not implemented. #4255 was independently confirmed fixable (and fixed) without this field — `classify_glass_into_material` runs immediately after `resolve_pbr()` in `material_translate.rs`, before any other consumer touches `material_kind`, so the original authored value was already reliably present at the point the fix's guard needs it; no read-back into `ImportedMaterial` was required. This issue's own text states it has "no independent runtime effect beyond enabling the companion HIGH finding," which is now closed by #4255's own guard. Remaining scope here is a genuine architectural improvement (new canonical field + a SIBLING audit of `TextureSlotContext`'s existing raw-tier read-back) that widens the fix surface beyond this batch's per-issue scope; deferred rather than folded in speculatively.
