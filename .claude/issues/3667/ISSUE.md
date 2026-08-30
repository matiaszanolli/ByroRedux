# #3667 — PERF-D5-2026-08-30-02: `copy_depth_to_history` runs a full-render-resolution depth copy plus four layout barriers every frame for a feature most frames don't use, and no GPU timer covers it

- **Source**: `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md`
- **Finding ID**: `PERF-D5-2026-08-30-02`
- **Filed**: 2026-08-30 (HEAD `64f64480`)
- **Labels**: medium,performance,renderer,pipeline,bug
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3667

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is authoritative for current state.

---

- **Severity**: MEDIUM
- **Dimension**: GPU Pipeline
- **Location**: `crates/renderer/src/vulkan/context/post_passes.rs:54-145`
  (`copy_depth_to_history`); unconditional call site
  `crates/renderer/src/vulkan/context/draw.rs:3676`
- **Status**: NEW
- **Description**: Immediately after the main render pass, every frame, the engine
  transitions the depth image `DEPTH_STENCIL_READ_ONLY_OPTIMAL → TRANSFER_SRC`,
  the history image `SHADER_READ_ONLY → TRANSFER_DST`, issues a full-extent
  `vkCmdCopyImage` of the D32 depth buffer, then transitions both back. The sole
  consumer is the soft-particle depth fade in `crates/renderer/shaders/triangle.frag:970-983`,
  which is itself gated on `(mat.materialFlags & MAT_FLAG_EFFECT_SOFT) != 0u &&
  mat.softFalloffDepth > 0.0`. A scene with no soft-effect material pays the entire
  copy for a texture nothing samples.
- **Evidence**: `draw.rs:3676` calls it with no predicate of any kind:
  ```rust
  self.copy_depth_to_history(cmd);
  ```
  The shader-side gate is the *only* read of `depthHistoryTex`
  (`crates/renderer/shaders/include/bindings.glsl:333` declares it; `triangle.frag:973`
  is the single `texture(depthHistoryTex, …)` call site).
- **Impact**: Derived from the checked-in `D32_SFLOAT` depth format and
  `frame_extents.render`: 8.3 MB read + 8.3 MB write per frame at 1920×1080,
  33.2 MB each way at native 3840×2160 — plus two full `vkCmdPipelineBarrier`
  pairs that sit directly between the render pass and the whole post chain. This
  is also the **only per-frame GPU pass in the frame with no `gpu_timers`
  bracket**: it falls in the gap between `cmd_main_render_end` and
  `cmd_svgf_start`, so it is invisible to the console/bench per-pass summary and
  cannot be measured today without adding an instrument. That is the same
  measurement-gap shape as `PERF-2026-08-27b-02`.
- **Related**: `PERF-2026-08-27b-02` (a pass whose own doc defers tuning to a
  measurement no timer can produce). The `caustic_splat` pass next door already
  demonstrates the CPU-side skip pattern this wants.
- **Suggested Fix**: Two independent, cheap steps. (1) Add a
  `copy_depth_to_history` bracket to `GpuPerFrameTimers` (the query pool grows
  28→30, `active_bits` has spare bits) so the cost is measurable before anything
  is changed. (2) Gate the copy on a scene-level "any loaded material carries
  `MAT_FLAG_EFFECT_SOFT`" bit rather than a per-frame draw-list scan — a
  per-frame predicate would leave a newly-appearing FX sampling arbitrarily stale
  depth on its first frame, whereas a scene-level bit cannot. The skip is
  layout-neutral: the helper leaves the depth image in the same
  `DEPTH_STENCIL_READ_ONLY_OPTIMAL` it found it in, which is also the precondition
  `depth_capture_record_copy` documents on the very next line — so omitting the
  call cannot break the layout contract of anything downstream. Confidence on the
  layout-neutrality argument: high (in/out layouts are literally identical); on
  the magnitude: unmeasured, which is what step (1) is for.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md` (HEAD `64f64480`). Report status: NEW; re-verified CONFIRMED against HEAD at publish time.*
