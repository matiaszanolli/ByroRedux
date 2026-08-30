# #3605 — REN-2026-08-30-D13-02: TAA's permanent-failure latch signals no temporal discontinuity, unlike FSR's `#2519` edge — the failing frame is rendered jittered and blitted unresolved

**Labels**: `low,renderer,bug`
**Filed**: 2026-08-30 via `/audit-publish`
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-30.md`

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is
> authoritative for current state — `gh issue view 3605 --json state`.

---

- **Severity**: LOW
- **Dimension**: TAA
- **Location**: `crates/renderer/src/vulkan/context/post_passes.rs` (`record_taa_pass`, lines 753–778) vs. the FSR sibling at lines 1026–1036 (`take_new_dispatch_failure` → `signal_temporal_discontinuity`)
- **Status**: OPEN — dormant today (see Impact), asymmetry with a hazard the FSR side explicitly closes
- **Description**: `taa_jitter` is evaluated at the top of `draw_frame` (`draw.rs:2039–2048`) and gates on `self.taa_failed`, so once the latch is set every *subsequent* frame renders unjittered — that is `#1932`. But `taa_failed` is set inside `record_taa_pass`, in the post-pass tail, long after the geometry pass for that frame already rendered with the Halton offset. Composite is then rebound to raw HDR (`fall_back_to_raw_hdr`) and that jittered image is presented with nothing to resolve it, and, more importantly, the *next* frame's SVGF / volumetrics reprojection accumulates against G-buffer content that is half a pixel offset from everything after it. The FSR path treats exactly this as a hazard worth a one-shot signal: `FrameUpscaler::new_dispatch_failure` (`frame_upscaler.rs:113–125`) → `take_new_dispatch_failure()` → `self.signal_temporal_discontinuity(FSR_DISPATCH_FAILURE_RECOVERY_FRAMES)`. Its own doc names the TAA side as the same class of hazard, but `record_taa_pass` calls no equivalent.
- **Evidence**:
  - `post_passes.rs:763–771`: on `Err`, the handler does `self.taa_failed = true;` and `composite.fall_back_to_raw_hdr(&self.device);` — nothing else. `grep -rn "self.taa_failed = true" crates/renderer/src` returns this one site.
  - `post_passes.rs:1032–1036`: `if fsr_dispatch_failed_this_frame { self.signal_temporal_discontinuity(...); }` — the guard TAA lacks.
  - `frame_upscaler.rs:122–125`: "Same class of hazard `taa_jitter`'s `!taa_failed` gate closes on the TAA side (#1932)" — the `#1932` gate covers later frames, not the failing frame.
  - Adjacent, same shape: `draw.rs:3559–3564` logs `TAA upload_params failed` and continues, so the dispatch still runs against `param_buffers[frame]`'s contents from two frames ago (stale `screen` / `first_frame`). `GpuBuffer::write_mapped` (`buffer.rs:1160–1173`) can only fail on a missing/unmapped allocation, so this is theoretical rather than reachable.
- **Impact**: Currently **dormant**: `TaaPipeline::dispatch` (`taa.rs`) contains no fallible call between its `cmd_pipeline_barrier` prologue and its terminal `Ok(())`, so the `Err` arm in `record_taa_pass` is unreachable and `taa_failed` can never latch from it. The finding is a defence-in-depth gap that opens the moment anything fallible (a descriptor rewrite, a per-dispatch UBO write, a device-lost probe) is added to `dispatch`, at which point the failing frame silently poisons one frame of every downstream temporal history.
- **Suggested Fix**: In `record_taa_pass`'s `Err` arm, add `self.signal_temporal_discontinuity(N)` alongside the existing latch + `fall_back_to_raw_hdr`, mirroring `post_passes.rs:1032`. A named constant next to `FSR_DISPATCH_FAILURE_RECOVERY_FRAMES` keeps the two recovery windows discoverable together. Pin it with a source-scan test in the style of `frame_upscaler.rs:1302` (`POST_PASSES_RS.contains("take_new_dispatch_failure()")`).

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D13-02

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
