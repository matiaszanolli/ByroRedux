# Issues 3246, 3247, 3282, 3305

## #3246 — D7-01: Animated material sinks hash raw per-frame float bits into dedup key with no quantization
Domain: renderer (byroredux-renderer)
Severity: medium (performance)
Location: `byroredux/src/render/static_meshes.rs` (collect_static_mesh_draws), `crates/renderer/src/vulkan/material.rs` (hash_gpu_material_fields)
Fix: quantize animated material float fields before hashing, mirroring `quantize_fade`/`COLOR_FADE_STEPS` in particles.rs (#1795).

## #3247 — D23-01: Bloom-relocation onto FSR color-input path introduced unvalidated barriers around scene_color
Domain: renderer (byroredux-renderer)
Severity: medium, HYPOTHESIS only — needs BYRO_VALIDATION=1 / RenderDoc confirmation, no code fix proposed.
Location: crates/renderer/src/vulkan/bloom.rs::apply_to_scene (~760-851), post_passes.rs::record_bloom_pass (~880-905)

## #3282 — TD1-2026-08-24-01: draw_frame re-grew to 2498 LOC - 51% of a 4909-line file
Domain: renderer (byroredux-renderer)
Severity: low, tech-debt
Location: crates/renderer/src/vulkan/context/draw.rs:1522-4020 (draw_frame)
Fix: mechanical extraction into helpers, mirroring record_geometry_pass/record_post_passes (#2258/#2259). Preserve barrier/dispatch order verbatim.

## #3305 — REN-2026-08-26-01: dynamic actors (creatures) show no ground-contact shadow
Domain: renderer (byroredux-renderer)
Severity: medium, needs RenderDoc capture — investigation only, no fix proposed.
Location: shadow masking (lights.rs, predicates.rs, lighting.rs), skinned BLAS refit timing (blas_skinned.rs)
