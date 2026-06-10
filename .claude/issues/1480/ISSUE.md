# #1480 — REN-D22-NEW-01: Per-draw roughness re-classification overrides canonical Material.roughness

_Snapshot as filed 2026-06-09 from docs/audits/AUDIT_RENDERER_2026-06-09.md. GitHub is authoritative for live state._

**Severity**: MEDIUM (NIFAL contract drift + observability; output in-range, no GPU hazard)
**Dimension**: NIFAL Material Translation
**Source**: `docs/audits/AUDIT_RENDERER_2026-06-09.md`
**Status**: NEW

## Description
The static-mesh render path re-derives the canonical `Material.roughness` **per draw**, violating the NIFAL "resolve-once, no render-time heuristic" contract. The `mut roughness` local is seeded correctly from canonical `m.roughness` (`byroredux/src/render/static_meshes.rs:304`) but is then **overwritten at render time** for any draw matching the `normal-alpha-as-spec` gate `material_kind < 100 && metalness < 0.3 && env_map_scale <= 0.3 && normal_map_index != 0 && gloss_map_index == 0`:
- `static_meshes.rs:402` — `roughness = (1.0 - glossiness / 100.0).clamp(0.05, 0.95)` (recomputed from `m.glossiness`).
- `static_meshes.rs:404` — `roughness = (0.85 - (specular_strength - 1.0) * 0.1).clamp(0.4, 0.85)` (recomputed from `m.specular_strength`).

Both `glossiness` and `specular_strength` are canonical fields `resolve_pbr()` already consumed at the translate boundary. The mutated value flows `:575` → `DrawCommand::to_gpu_material()` (`crates/renderer/src/vulkan/context/mod.rs:388`) → `GpuMaterial.roughness` → `MaterialTable::intern`, so the value the material table receives is **not** the canonical translate output for this population.

## Nuance (verified)
The gate genuinely depends on **render-side texture-resolution facts** — `normal_has_alpha` and the resolved `normal_map_index`/`gloss_map_index` (TextureRegistry handles, unavailable at NIF-import translate time). So the *condition* cannot move to translate wholesale; only the **roughness scalar derivation** (which uses translate-available `glossiness`/`specular_strength`) can. This is **not** a naive "move the block to translate" fix.

## Evidence
- `crates/core/src/ecs/components/material.rs:218-223` documents the broken contract: roughness "same resolve-once contract… The renderer reads this as `GpuMaterial.roughness` directly — no shader-side branching."
- `byroredux/src/material_translate.rs:158` calls `resolve_pbr()` once; nothing re-resolves until `static_meshes.rs:402/404` mutates the value mid-frame.
- Prior audits' "no per-draw classify" claims are narrowly true (the *keyword* classifier is gone) but missed this arm — they checked the block at `static_meshes.rs:330-348`, just above this one.

## Impact
For the gated population (the bulk of Skyrim/Oblivion architecture/clutter — lit, normal-mapped, no dedicated gloss map, `env_map_scale≈0`, `metalness<0.3`), the canonical resolved roughness is silently discarded and replaced by a render-time value; `material_dump`/`mat.*` tooling reports a roughness the GPU never uses for these meshes. Output is clamped/in-range — no GPU hazard. Re-introduces a slice of the per-draw material work the NIFAL refactor claimed to delete.

## Suggested Fix
Resolve `normal_has_alpha` once when the normal map is attached (spawn / texture-load), write the derived roughness back into `Material.roughness`, and have `static_meshes.rs` only set the `NORMAL_ALPHA_SPEC_BIT` gloss-map flag (the per-pixel modulation is legitimately a shader concern) — never recompute the scalar per-frame.

## Related
#1357 (BGSM_* flag aliases, open), `docs/engine/nifal.md`, `/audit-nifal`. See also the `feedback_format_translation` rule (per-game quirks resolve at the parser→`Material` boundary, never at render time).

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: the normal-alpha-as-spec roughness derivation is resolved at/near `translate_material` (or a one-shot texture-attach step), not in the render system; `Material.roughness` remains the single resolved value.
- [ ] **SIBLING**: audit the rest of `static_meshes.rs` (and any other render-data builder) for further per-draw mutation of canonical `Material` scalars (metalness, IOR, emissive).
- [ ] **TESTS**: regression test asserting the gated population's `GpuMaterial.roughness` equals the canonical `Material.roughness` (i.e. tooling-reported == GPU-used).
- [ ] **UNSAFE / DROP / LOCK_ORDER / FFI**: N/A.
