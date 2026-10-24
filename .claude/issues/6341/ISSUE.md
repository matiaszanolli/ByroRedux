# PERF-D5-03: Composite caustic fetches and the water-caustic clear run with or without caustic sources

- **Issue**: #6341 (snapshot as filed; GitHub is authoritative for live state)
- **Labels**: low, performance, renderer, shaders, bug
- **Source**: docs/audits/AUDIT_PERFORMANCE_2026-10-24.md

- **Severity**: LOW
- **Dimension**: GPU Pipeline
- **Location**: `crates/renderer/shaders/composite.frag:646-675` (the reads); `crates/renderer/src/vulkan/context/build_and_upload_instances.rs:1524` (`wca.clear_pre_render_pass`) and `:1266` (`water_caustic_active: self.post.water_caustic_accum.is_some()`); `crates/renderer/src/vulkan/water_caustic.rs:185`
- **Description**: two fixed per-frame costs scale with render pixels and do not depend on whether the scene has water or refractive glass.
  1. Every geometry pixel in `composite.frag` does three `texelFetch` on the glass accumulator (`causticTex`, 3 layers of R32_UINT) and one on the water accumulator (`waterCausticTex`). The only gate is `params.caustic_flags.x`, which the host sets from `water_caustic_accum.is_some()`, true for the whole session. There is no "no caustic content this frame" signal. When no source exists, `CausticPipeline::clear_for_skip` zeroes the glass accumulator once per skip streak, so the composite reads zeros.
  2. `WaterCausticAccum::clear_pre_render_pass` runs on every frame the accumulator exists (its own comment: "runs unconditionally every frame"): one render-extent R32_UINT clear plus two barriers before the main pass. The glass side has the skip-streak latch (`skip_clear_decision`); the water side has no equivalent.
  No pass-inventory row exists for either, and no timer brackets the clear, so a bench cannot see it.
- **Evidence**: `composite.frag:651-664` reads layers 0–2 and the water texture behind an always-set uniform flag; `memory-budget.md:375-390` (Glass + Water Caustics) records both accumulators at full render resolution, 4 B/px per layer (`CAUSTIC_FORMAT` R32_UINT).
- **Impact** (derived, not measured): reads 16 B/px and the water clear writes 4 B/px per frame: about 33 MB read plus 8 MB written at 1920×1080, about 166 MB at 3840×2160. L2 residency of the zero-filled reads is unknown, so the real cost may be much smaller. Treat as LOW until a `composite_ms` A/B on a no-water, no-glass cell shows at least 0.1 ms.
- **Suggested Fix**: derive one `caustic_content_live` flag (glass dispatched this frame or its decay history non-empty, or a water draw recorded for this slot) into `caustic_flags` and let `composite.frag` skip all four fetches when clear; gate the water clear with a once-per-streak latch like `skip_clear_decision`; add a pass-inventory row. **Speculative-Vulkan caveat**: the barrier change is invisible to `cargo test`, so it needs RenderDoc evidence or a revert plan before landing. Confidence: medium on existence, low on magnitude.


## Completeness Checks
- [ ] **A/B FIRST**: a pinned-tier `composite_ms` A/B on a no-water, no-glass scene shows >= 0.1 ms before any change lands (`--rt-test-ray-quality-tier N`, same camera pose, fresh `BYROREDUX_SETTINGS_PATH`)
- [ ] **DROP**: if Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **SYNC**: any barrier change has RenderDoc evidence or a revert plan (speculative-Vulkan rule)
- [ ] **SIBLING**: glass and water accumulators handled symmetrically; the skip-streak latch is rolled back on the tail `Err` sites like #5956
- [ ] **SHADER-SYNC**: composite uniform struct and `composite.frag` stay in lockstep (rebuild `.spv` with plain `-V`)
- [ ] **INVENTORY**: add the pass-inventory row and a `memory-budget.md` note
- [ ] **TESTS**: a pin test for the new flag

_Filed from `docs/audits/AUDIT_PERFORMANCE_2026-10-24.md`. Derived traffic only; no GPU time is claimed._
