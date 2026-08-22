# Batch fix: #2421, #2605, #2686, #2688

## #2421 — TD4-002: _audit-common.md shader count self-contradiction
`.claude/commands/_audit-common.md:63` says 21 GLSL sources (matches
`ls crates/renderer/shaders/*.{vert,frag,comp}`); `:104` says a different,
undercounted number. Doc-only fix: reconcile the two lines.
Domain: doc-only (audit skill file).

## #2605 — FO4-D5-05: BSVER 140-154 dead shader_type band undocumented
NIF parser forces `shader_type=0`/`None` for BSVER 140-154 (no shipping game
uses this band). Informational — add a comment noting this is intentional,
not an oversight. Domain: **nif** → `byroredux-nif`.

## #2686 — SAFE-D7-01: GLASS_RAY_BUDGET is a dead constant
`shader_constants_data::GLASS_RAY_BUDGET` (2_097_152) is mirrored into GLSL
but never referenced by any shader — the real runtime cap is
`AdaptiveRayBudget::settings()`'s 4 hard-coded tier literals in
`crates/renderer/src/vulkan/scene_buffer/ray_budget.rs`, none of which
reference the constant. Fix: derive the tier table from
`GLASS_RAY_BUDGET` (tier 3 = full budget, lower tiers as fractions), add a
test pinning them together. Domain: **renderer** → `byroredux-renderer`.

## #2688 — SAFE-D6-01: GLSL<->Rust GpuMaterial lockstep never checks scalar type
`parse_glsl_struct_fields` (gpu_instance_layout_tests.rs) discards the GLSL
type token, so a `uint`<->`float` flip in bindings.glsl would be invisible to
`cargo test`. Fix: capture `(type, name)` pairs and assert type equivalence
(`float`<->`f32`, `uint`<->`u32`) alongside the existing order/name checks.
Domain: **renderer** → `byroredux-renderer`.

## Classification
#2421: doc-only. #2605: `byroredux-nif`. #2686 / #2688: `byroredux-renderer`.
