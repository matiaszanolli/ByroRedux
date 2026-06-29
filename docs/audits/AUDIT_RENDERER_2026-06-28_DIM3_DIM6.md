# Renderer Audit — Dimensions 3 (GPU-Struct Layout) + 6 (NIFAL Material Translation)

- **Date**: 2026-06-28
- **Command**: `/audit-renderer 3 6` → `--focus 3,6 --depth deep`
- **Branch**: main (HEAD `f9cc691b`)
- **Method**: Orchestrator + 2 dimension agents (renderer-specialist), adversarial per-finding disproof, symbol-anchored verification against the live tree. Reference docs (`docs/engine/shader-pipeline.md`, `docs/engine/memory-budget.md`, `docs/engine/nifal.md`) treated as authoritative. Dedup baseline: `gh issue list` (31 open) + the most recent comprehensive sweep `AUDIT_RENDERER_2026-06-26.md` and today's NIFAL owner audit `AUDIT_NIFAL_2026-06-28.md`. Orchestrator independently re-verified the load-bearing claims (5-site `GpuInstance` lockstep, single `GpuMaterial` decl, current size-pin names, the two `translate_material` callers, both regression-pin tests).

---

## Executive Summary

| Severity | Dim 3 | Dim 6 | Total |
|---|---|---|---|
| CRITICAL | 0 | 0 | 0 |
| HIGH | 0 | 0 | 0 |
| MEDIUM | 0 | 0 | 0 |
| LOW | 0 | 0 | 0 |
| INFO | 0 | 0 | 0 |

**Zero findings across both dimensions.** Both are mature, heavily-pinned layers that the 2026-06-26 sweep declared "fully locked" (Dim 3) and "clean" (Dim 6); this deep pass confirms **no drift since then** and recasts every invariant as a verified regression guard. The value of this audit is the symbol-anchored coverage record below, not a defect list.

No struct drift, no 6th `GpuInstance` decl site, no second `GpuMaterial` copy, no non-scalar `GpuMaterial` field, no unzeroed pad feeding Hash/Eq, no hand-written shader flag, no third `translate_material` leak caller, no per-frame re-classification, and no per-game branch leaking into the renderer was found.

---

## RT Pipeline / GPU-Struct Assessment (Dim 3)

**Layout pins — fully locked, verified GREEN.**
- **Size pins** (match `shader-pipeline.md`): `gpu_instance_is_112_bytes_std430_compatible` (112 B), `gpu_camera_is_336_bytes` (336 B, the 320→336 `render_origin` growth, #1492), `gpu_material_size_is_300_bytes` (the 260→…→300 growth via #804/#1147/#1248/#1249/#1250). The retired `gpu_material_size_is_260_bytes` name is **gone from live code** (orchestrator-confirmed: 0 occurrences in `crates/renderer/src/`; survives only in historical audit `.md`). `.spv`-level pin `camera_ubo_size_matches_gpu_camera_in_every_shader` also green.
- **Per-field offset pins**: `gpu_material_field_offsets_match_shader_contract` (#806) is exhaustive (`offset_of!` on every named field, 0..296 across all vec4 slots — the within-vec4-reorder guard a size-only pin can't catch); `gpu_material_glsl_field_order_matches_rust_struct` (#1657) parses both `material.rs` and `bindings.glsl` to **75 identical ordered fields**.
- **`GpuMaterial` scalar-only**: all 75 fields `f32`/`u32`, no `[f32;3]`; the struct is fully scalar-packed with **no pad fields**, so byte `as_bytes()`/`PartialEq` over the 300-byte image is deterministic. `GpuInstance`'s two pads (`_pad_id0`, `_pad_albedo`) are explicitly zeroed in `Default` and at the draw-append site.
- **`GpuInstance` 5-site lockstep**: `grep -rl "struct GpuInstance" crates/renderer/shaders/` == exactly 5 (`include/bindings.glsl`, `triangle.vert`, `ui.vert`, `water.vert`, `caustic_splat.comp`) — orchestrator-confirmed; `struct GpuMaterial` == 1. Field order byte-identical across all 5; the recurring `ui.vert`/`water.vert` wrong-offset trap (#785/#1498) is clean, guarded by `ui_vert_reads_texture_index_from_instance_not_material_table` + `water_shaders_must_not_acquire_material_buffer_binding`.
- **Flag constants single-sourced** in `shader_constants_data.rs` → `shader_constants.glsl`: `INSTANCE_FLAG_*`, `MAT_FLAG_*` (bits 0–9; the `BGSM_*` shader prefix is gone post-#1357, only Rust-side `material_flag::BGSM_AUTHORED` bit 10 remains, deliberately un-mirrored), `MATERIAL_KIND_*`, the 13 `DBG_*` bits — every Rust↔shader value pin (`instance_flag_bits_match_scene_buffer_consts`, `material_flag_bits_match_material_consts`, `material_kind_matches_scene_buffer_consts`, `triangle_frag_dbg_bits_not_redeclared`) green.
- **Capacities** match `memory-budget.md`: `MAX_INSTANCES = 0x40000`, `MAX_MATERIALS = 16384`, `MAX_INDIRECT_DRAWS = MAX_INSTANCES`; the 24-bit `instance_custom_index` const-assert (`MAX_INSTANCES < 1<<24`) is present; over-cap intern → id 0 + one-shot `warn!` (#797); upload truncates `min(len, MAX_MATERIALS)`.

**Drift check since 2026-06-26** (`git log` over all layout-relevant files): 3 commits, none structural — `1607e90c` (#1755, corrected a stale `gpu_material_size_is_260_bytes` comment cite in `bindings.glsl`, TD3-002, comment-only), `fd483a2f` (#1758, skin workgroup size through generated constants, unrelated), `eb71bcb9` (pex cleanup; the #1627 `glass()` TODO is the already-tracked renderer tech-debt item, not re-filed).

## NIFAL Material Assessment (Dim 6)

**Translation boundary — clean, verified as regression guards.**
- **Single boundary**: `translate_material` (`byroredux/src/material_translate.rs`, `pub(crate)`) has exactly **two** production call sites — `byroredux/src/scene/nif_loader.rs:796` (loose NIF) and `byroredux/src/cell_loader/spawn.rs:880` (REFR placement), orchestrator-confirmed. The only other `Material {…}` literals are the Cornell reference scene (`byroredux/src/cornell.rs`, routed through the same `MaterialTable`) and `#[cfg(test)]` fixtures. The `pub(crate)` visibility structurally prevents an external-crate leak.
- **`resolve_pbr` resolve-once + idempotent**: `Material::metalness`/`roughness` are plain `f32`; `translate_material` seeds them from `*_override.unwrap_or(NaN)` then calls `resolve_pbr()` once (NaN-sentinel → `classify_pbr_keyword`, clamp `metalness 0..1` / `roughness 0.04..1`). Pinned by `resolve_pbr_is_idempotent` (`material.rs:1009`, orchestrator-confirmed). No per-frame re-classification: `render/static_meshes.rs` reads `m.roughness`/`m.metalness` directly with an in-source "no per-draw keyword scan" note, and the legacy per-draw `Material::classify_pbr` is deleted. (`resolve_normal_alpha_spec_roughness` is a resolve-at-spawn write derived from glossiness/specular, NaN-guarded #1535, idempotent — not a per-frame mutation.)
- **`EmissiveSource`** (None/Material/Lighting/Effect) is tagged upstream (NIF inline `material/walker.rs`; BGEM merge `asset_provider/material.rs` → `EmissiveSource::Effect`, #1358) and copied through `translate_material`; the renderer's `to_gpu_material` reads the resolved `emissive_mult` scalar and never inspects `emissive_source` (zero reads across the render path). The Effect-variant diffuse-tint conflation is **deferred, not dropped** — matches the NIFAL ledger's parked-not-leak `base_color_scale` + emissive-normalization-no-op entries.
- **No per-game branch** between `Material` and `MaterialTable::intern`: `to_gpu_material` is a flat field copy, `intern` is game-agnostic; grep for `Game::`/`match game`/per-title arms across `crates/renderer/src/` returns zero control-flow branches. Per-game quirks resolve inside `translate_material` (BGSM/BGEM flag packing, classifier keyword arms), honoring `feedback_format_translation.md`.
- **Particle slice**: `apply_emitter_params` overrides kinematics + size (`start_size = end_size = initial_radius × base_scale`, #1775 spread) but deliberately NOT color (preset flame colours owned by `NiPSysColorModifier`); `emit_particles` reads the emitter post-overlay. Pinned by `apply_emitter_params_overrides_kinematics_and_size_not_color` (`particle.rs:456`, orchestrator-confirmed).

---

## Dedup ledger honored (checked, excluded — not re-reported)

- **Dim 3**: #1627 (`GpuMaterial::glass()` transmission TODO names a closed issue — already-tracked renderer tech-debt). The 2026-06-26 "fully locked" assessment confirmed unchanged.
- **Dim 6 parked-not-leak** (from `AUDIT_NIFAL_2026-06-28.md` documented-limitation ledger): emissive-normalization resolved-no-op (`nifal.md` §4), particle size-over-life curve (#1402), `base_color_scale` Effect deferral. **Out of scope / different layer**: D6-01 (collision-shape `bhkPackedNiTriStripsShape.Scale` — NIFAL collision slice, not material). **Already-tracked open issues**: #1333 (modern `NiParticleSystem` local transform, import-side), #1580 (BGEM `grayscale_to_palette_alpha` not forwarded), #1659/#1711/#1718/#1753.

## Prioritized Fix Order

None. Both dimensions are clean; no action required.

## Needs-RenderDoc

None. No sync/barrier or invisible-failure-mode observation arose in either dimension.

## Conclusion

Dimensions 3 and 6 remain **fully locked / clean** with no drift since the 2026-06-26 sweep. All ~30 layout pins (Dim 3) and 5 boundary invariants (Dim 6) verified to hold under symbol-level scrutiny, corroborated by an independent orchestrator spot-check and (Dim 3) a green run of the renderer pin suite. Nothing to publish.
