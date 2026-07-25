# 2165: D2-01: Two-sided alpha-blend split re-enabled for z_write=false batches — regression of #1804, corroborated on 3 game corpora at runtime

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/2165
**Labels**: bug, medium, performance

---

## Severity
MEDIUM

## Dimension
Draw-Call & Instancing Efficiency (Dim 2) — `/audit-performance` 2026-07-25, corroborated by `/audit-runtime` 2026-07-25

## Location
`crates/renderer/src/vulkan/context/draw.rs:325-328`; consumed at `crates/renderer/src/vulkan/context/geometry_pass.rs:323,394-407`

## Status
**Regression of #1804** ("two-sided glass split runs on additive particle batches — 2x draws + a fully-culled vertex pass with zero compositing benefit", CLOSED).

## Description
#1804 gated the two-pass FRONT-then-BACK cull split on `z_write`, since the split's purpose (back faces write depth before front faces blend) is meaningless when neither pass writes depth. Commit `883f57cd` (2026-07-20, "introduce stable surface ID") removed the `&& b.z_write` limb because FO4 BGEM glass is commonly authored `z_write == false` and a single `CULL_NONE` draw let TAA jitter pick a different blend winner per frame (a legitimate crawling-cross-hatch fix). But `z_write` was being used as a *proxy* for "order-dependent glass", and dropping the limb re-broadens the split to every two-sided blended batch — exactly the particle population #1804 excluded. The regression guard tests were **inverted rather than removed** (`draw.rs:3187-3192`, `splits_when_z_write_false`), so `cargo test` stays green through the regression.

Confirmed against current code:
```rust
// draw.rs:325-328 (current) — pre-883f57cd: `is_blend && b.two_sided && b.z_write`
pub(super) fn needs_two_sided_blend_split(b: &DrawBatch) -> bool {
    let is_blend = matches!(b.pipeline_key, PipelineKey::Blended { .. });
    is_blend && b.two_sided
}
```
Particle draws qualify on every limb (`byroredux/src/render/particles.rs:130,133,210`: `alpha_blend: true`, `two_sided: true`, `z_write: false`). The consumer branch also exits the indirect path entirely (`geometry_pass.rs:394-407`): two direct `cmd_draw_indexed` calls plus two `cmd_set_cull_mode` instead of one `cmd_draw_indexed_indirect` group.

## Impact
Every two-sided blended particle batch costs 2 direct draws instead of participating in one indirect group, and the FRONT-cull pass runs the full instanced vertex walk to produce zero camera-facing fragments (billboards are front-facing by construction). Batch count is small (particles collapse to ~1 batch per distinct blend combo), so blast radius is bounded — wasted work, not a stall.

**Independently corroborated by the concurrent `/audit-runtime` sweep across three separate game corpora** (`docs/audits/AUDIT_RUNTIME_2026-07-25.md` RT-1/RT-2/RT-3), confirming the regression is real and visible in actual per-frame draw-call telemetry, not just a static code read:
- **fnv `FreesideAtomicWrangler`**: `bench_draws_gpu_calls` 10 -> 23 (+130%, exceeds x1.1 tolerance), cmds/batches essentially flat.
- **oblivion `ICMarketDistrictTheGildedCarafe`**: `bench_draws_batches` 27 -> 31 (+14.8%, exceeds x1.1 tolerance); this exact cell was an **exact** match (`324/27b/4c`) as recently as the 2026-07-16 runtime sweep, bracketing the drift to the 2026-07-20 `883f57cd` window.
- **fo4 `InstituteBioScience`**: `bench_draws_gpu_calls` 40 -> 46 (+15%, exceeds x1.1 tolerance).

All three games' regressions trace to the identical root cause (particle FX batches — `fxmistlow01`/`fxsmokewisps01`-style ambient effects on FNV, torch/campfire additive particles on Oblivion, ambient/FX emitters on FO4 — now failing the split-eligibility check and losing indirect grouping). No separate issues needed for RT-1/RT-2/RT-3; they are runtime telemetry confirmation of this same regression, not independent bugs.

## Related
#1804 (closed, reverted); commit `883f57cd`; `docs/audits/AUDIT_PERFORMANCE_2026-07-19.md`; `docs/audits/AUDIT_RUNTIME_2026-07-25.md` RT-1/RT-2/RT-3 (runtime corroboration, merged here).

## Suggested Fix
Stop using `z_write` as the glass proxy. Carry an explicit `two_sided_blend_split: bool` on `DrawCommand`/`DrawBatch`, set at emit time from `material_kind` (`MATERIAL_KIND_GLASS` or MultiLayerParallax) — preserves the FO4 BGEM fix exactly while restoring the particle fast path, and re-point the (currently inverted) unit tests at a predicate that actually distinguishes the two populations.

## Completeness Checks
- [ ] **TESTS**: Un-invert `splits_when_z_write_false` (or replace with a correctly-signed test) so `cargo test` catches this class of regression again
- [ ] **SIBLING**: Verify the fix restores the exact baseline `bench_draws_*` numbers on fnv/oblivion/fo4 per the runtime baselines
