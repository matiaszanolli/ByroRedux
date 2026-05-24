# AUDIT_PERFORMANCE — Dimension 7: TAA & GPU Skinning Cost (2026-05-24)

## Executive Summary

- **4 findings**: 0 CRITICAL · 0 HIGH · 0 MEDIUM · 4 LOW · 4 INFO (1 NEW + 3 carry-forward LOW from 2026-05-19; 4 INFO are re-verifications of closed fixes)
- **Frame-time impact**: ~0 ms — the four MEDIUM findings from the 2026-05-19 sweep all shipped (#1195/#1196/#1197 wired, #1127 closed-as-stale-premise). The remaining LOWs sum to cosmetic / resize-edge churn only.
- **VRAM impact**: ~0 MB — `shrink_blas_scratch_to_fit` is wired at `cell_loader/unload.rs:200` + `vulkan/context/resize.rs:41` and walks the shared scratch (used by both static AND skinned BLAS); residual mid-cell-camp pin acknowledged in #1127's stale-premise closeout.
- **Baseline landed cleanly**: every MEDIUM-tier action item from 2026-05-19 verified present in current source. Zero regressions detected.

### Dedup notes vs prior audits

- **#1195 / PERF-DIM7-01** (skin compute dispatch per-entity per-frame gate) — **CLOSED, shipped via commit `57c34c7f`** (2026-05-22). Verified at `skin_compute.rs:88-99,1018-1044` + `draw.rs:1025-1027`. ✓
- **#1196 / PERF-DIM7-02** (BLAS refit unconditional gate) — **CLOSED, same commit `57c34c7f`**. Verified at `draw.rs:1149-1175` paired with `blas_skinned.rs:578` (`has_skinned_blas`). ✓
- **#1197 / PERF-DIM7-03** (per-dispatch descriptor-set rewrite) — **CLOSED, commit `946e95f9`** (2026-05-23). Verified at `skin_compute.rs:640-650` (`SkinPaletteComputePipeline`) + `skin_compute.rs:195-209` (`SkinComputePipeline`). ✓
- **#1127 / PERF-DIM7-04 / REN-D2-NEW-01** (skinned BLAS scratch shrink) — **CLOSED as stale-premise** 2026-05-24T01:57:43Z. Verified: `shrink_blas_scratch_to_fit` IS wired (call sites at `byroredux/src/cell_loader/unload.rs:200`, `crates/renderer/src/vulkan/context/resize.rs:41`). Skinned + static BLAS share `self.blas_scratch_buffer` (`blas_skinned.rs:193-208`), so the static-survivors peak walk at `memory.rs:52-58` is a correct lower-bound after the unload drops all skinned entries. ✓
- **PERF-DIM7-07** (`MAX_PENDING_BIND_INVERSE_UPLOADS_PER_FRAME` bump 16→227) — **SHIPPED**: `crates/renderer/src/vulkan/scene_buffer/constants.rs:54` reads `227`. The 2026-05-19 LOW recommendation landed. ✓
- **#1194 / PERF-DIM7-INSTR** (per-pass GPU timer + `dispatches_skipped` counter) — **SHIPPED** via commit `e5774b19`. `self.gpu_timers` wired across `draw.rs:254,1003,1049,1139,1217,2733,2745`; `SkinCoverageFrame.dispatches_skipped` now incremented by #1195's skip path.

### Infrastructure gap (carried forward)

- **dhat / alloc-counter regression coverage** still NOT wired (carried since 2026-05-04). None of the Dim 7 findings here are alloc-hot-path — they are GPU-dispatch and VRAM-residency. The gap does not directly bear on Dim 7 numbers.
- **No regression test for the FNV-1a `pose_hash` first-sight invariant** today; `byroredux/src/render/skinned.rs` covers identity / single-bit / empty-slice but not "fresh entity always returns dirty." Unit-test exists at `crates/core/src/ecs/resources.rs` per the commit body (`first_sight_pose_is_always_dirty`), so this is INFO-level coverage.

---

## Hot Path Analysis (per-frame, Prospector baseline, 34 skinned NPCs / ~20 idle)

| Pass | Per-frame ops | Status |
| --- | --- | --- |
| TAA dispatch | 22 taps × pixels, single full-screen compute (O(pixels) only) | Clean — verified at `taa.comp` |
| TAA history buffers | 2 × RGBA16F @ swapchain res (~64 MB at 4K, ~12 MB at 1440p) | Resize-only churn — PERF-DIM7-05 (LOW carry) |
| TAA initial layout | Single UNDEFINED→GENERAL barrier at `initialize_layouts` post-`new()` | Clean — verified `taa.rs:604-637` |
| Skin compute dispatch | 34 dispatches × ~5K verts MAX, **bone-pose dirty-gated** | Clean — #1195 wired |
| Skinned BLAS refit (UPDATE) | 34 refits MAX, **paired-gated with #1195 + `has_skinned_blas` accessor** | Clean — #1196 wired |
| Descriptor writes (skin) | 0 in steady state (3 cold first-dispatch per FIF only) | Clean — #1197 wired |
| Pose-dirty book-keeping | FNV-1a over bone-slice f32s (~0.5 µs / entity at 32 bones) | Clean — `byroredux/src/render/skinned.rs:152,179-180` |
| BLAS shared scratch | Grows via `scratch_needs_growth`; shrinks at cell-unload + resize | Clean — wired #1127 closeout |
| Bone palette `bone_world` | MBPM=144 stride × slot; ~120-160 KB/frame zero-pad | Residual — PERF-DIM7-09 (LOW carry) |
| Bone palette `bind_inverses` | Persistent SSBO (M29.6) — single seed-once write per slot | Clean |
| Pending bind_inverses upload | Cap = 227 (was 16; #1191/-2/-3 bump landed) | Clean — PERF-DIM7-07 INFO |
| Skin output buffer | Lazy alloc, despawn-bounded; bumped on first dispatch | Clean — `has_populated_output: bool` guard |
| GPU-pass timestamps | `self.gpu_timers` Option live at 7 sample sites in `draw.rs` | Clean — #1194 wired |
| M29.3 raster fast-path | Triangle.vert still inlines weighted-matrix sum | Deferred (per plan) — INFO |

---

## Verifications (closed findings re-confirmed in current source)

- **#1195 / PERF-DIM7-01** — `SkinSlot.has_populated_output` flag introduced at `crates/renderer/src/vulkan/skin_compute.rs:88-99`; `pose_dirty: HashSet<EntityId>` on `SkinSlotPool` at `crates/core/src/ecs/resources.rs:544`. Skip gate at `crates/renderer/src/vulkan/context/draw.rs:1025-1027` reads `slot.has_populated_output && !is_dirty` → increments `last_skin_coverage_frame.dispatches_skipped`. LRU bump (`slot.last_used_frame`) precedes skip gate — quiescent slots NOT reaped. ✓ SHIPPED.
- **#1196 / PERF-DIM7-02** — Paired refit skip at `crates/renderer/src/vulkan/context/draw.rs:1167-1175`: `slot.has_populated_output && !is_dirty && accel.has_skinned_blas(entity_id)` → BLAS refit suppressed. `has_skinned_blas` accessor added at `crates/renderer/src/vulkan/acceleration/blas_skinned.rs:578`. First-sight BUILD path falls through correctly. ✓ SHIPPED.
- **#1197 / PERF-DIM7-03** — `SkinComputePipeline.descriptor_writes_this_frame: Cell<u32>` at `crates/renderer/src/vulkan/skin_compute.rs:209`; per-FIF cache key `(input, palette)` per `SkinSlot.descriptor_bindings` (see doc-comment at lines 95-101). Palette pipeline mirror at `skin_compute.rs:640-650` with cache key `(bone_world, bind_inverse, palette)`. Steady-state writes = 0 after `MAX_FRAMES_IN_FLIGHT` warm-up. ✓ SHIPPED.
- **#1127 / PERF-DIM7-04 / REN-D2-NEW-01** — Closed as stale-premise. `shrink_blas_scratch_to_fit` IS wired (verified call sites). Shared scratch (used by both static + skinned BLAS via `self.blas_scratch_buffer`) cycles at cell unload + swapchain resize. Residual mid-cell pin is intentional deferral (mid-frame deferred-buffer-destroy infra ~100-200 LOC, 16-32 MB worst case on 12 GB VRAM = 0.27% — not justified at LOW severity). ✓ CORRECTLY CLOSED.
- **#1194 / PERF-DIM7-INSTR** — Per-pass GPU timer infrastructure live (`self.gpu_timers: Option<…>` checked at 7 sites in `draw.rs`). `SkinCoverageFrame.dispatches_skipped: u32` at `skin_compute.rs:138`; surfaced through `skin.coverage` console command. ✓ SHIPPED.
- **TAA `frames_since_creation` reset on resize** — verified at `taa.rs:809`. Pairs with `signal_history_reset()` for cell-load (#801) and recovery-α SVGF window. ✓
- **TAA initialize_layouts** uses `PipelineStageFlags::NONE` (Vulkan 1.3) per #949/#1100/#1122. ✓

---

## Findings — by Severity

### LOW

#### PERF-DIM7-14: Stale `dispatches_skipped` doc-comment claims gate "not yet landed" — gate shipped, comment didn't update
- **File**: `crates/renderer/src/vulkan/skin_compute.rs:128-134`
- **Severity**: LOW (cosmetic / doc rot only — no behavior impact)
- **Status**: NEW
- **Cause**: When #1194 (PERF-DIM7-INSTR) landed (2026-05-21, commit `e5774b19`), the counter was pre-staged with a forward-looking comment: *"Today the value is always zero; instrumentation pre-staged so the dirty-gate commit drops in cleanly."* The dirty-gate commit (#1195, `57c34c7f`) then landed 2026-05-22 and the counter is now incremented — but the comment was not updated.
- **Fix**: Rewrite lines 128-134 to read "When the bone pose is unchanged for an entity this frame, `dispatches_skipped` increments and the compute dispatch is suppressed (#1195 / PERF-DIM7-01)." Drop the "always zero" sentence. ~3 lines.
- **Estimated Impact**: 0 ms / 0 MB. Doc rot only — but a stale claim like "always zero" actively confuses readers triaging the next perf audit.

#### PERF-DIM7-05: TAA history image resize destroys + reallocates 2× full RGBA16F (CARRY-FORWARD-FROM-2026-05-19)
- **File**: `crates/renderer/src/vulkan/taa.rs:793-826` (`recreate_on_resize`)
- **Severity**: LOW
- **Status**: CARRY-FORWARD-FROM-2026-05-19. Identical behavior — no progress since.
- **Cause**: `recreate_on_resize` drains the entire `self.history` vec (line 793), destroys every image view + image + frees every allocation, then creates `MAX_FRAMES_IN_FLIGHT` fresh `create_history_image` calls (line 815-826). At 4K this is ~64 MB of fresh allocation per resize event.
- **Fix**: Image reuse pool keyed on `(width, height, format)` — same idea as the existing static-mesh buffer reuse path. ~50 LOC, MEDIUM risk (allocator integration).
- **Estimated Impact**: ~5-15 ms one-shot hitch on resize at 4K. Resize is rare in a session, so this is genuinely LOW.

#### PERF-DIM7-08: TAA history descriptor write touches slot 0 in UNDEFINED layout on frame 0 (CARRY-FORWARD-FROM-2026-05-19)
- **File**: `crates/renderer/src/vulkan/taa.rs` (init path) + `initialize_layouts` at line 604
- **Severity**: LOW (defensive concern; functionally correct today)
- **Status**: CARRY-FORWARD-FROM-2026-05-19. No change in current source.
- **Cause**: Descriptor write happens during `new_inner` before the explicit `UNDEFINED → GENERAL` barrier in `initialize_layouts`. This is legal Vulkan (writing a descriptor pointing at an image in UNDEFINED is fine; the shader read is what requires the layout transition). The `should_force_history_reset(c) := c < MAX_FRAMES_IN_FLIGHT` gate at `taa.rs:109` ensures the first MAX_FRAMES_IN_FLIGHT dispatches read no history. So the actual UNDEFINED memory is never read by a shader.
- **Fix**: Either (a) move descriptor write past `initialize_layouts`, or (b) add a `debug_assert` that asserts the layout is GENERAL at first sampling. Both <10 LOC.
- **Estimated Impact**: 0 ms today. Pinning by `debug_assert` is the cheapest path; preserves the "guarded but fragile" status as "guarded and pinned".

#### PERF-DIM7-09: Bone palette MBPM-strided post-M29.6; partial poses pay full 144-slot zero-pad (CARRY-FORWARD-FROM-2026-05-19)
- **File**: `crates/renderer/src/vulkan/skin_compute.rs:319-325` (MBPM=144 constant); upload path in `crates/renderer/src/vulkan/scene_buffer/upload.rs`
- **Severity**: LOW
- **Status**: CARRY-FORWARD-FROM-2026-05-19 — deferred to "dedicated M29.x milestone" by prior audit; that decision still stands. No regression.
- **Cause**: `MAX_BONES_PER_MESH = 144` (#1135) — every skinned slot's `bone_world` row consumes 144 mat4s regardless of the entity's actual bone count (which can be 20-50 for many NPCs). Per-frame zero-pad bandwidth: ~120-160 KB on Prospector (34 slots × ~80 unused bones × 64 B).
- **Fix**: Variable-stride palette with a per-entity bone-count header. Touches palette compute shader + every upload site + descriptor binding. ~300-500 LOC. Not a hotfix — defer to M29.x.
- **Estimated Impact**: ~120-160 KB/frame bandwidth recoverable. <1% of total per-frame upload traffic. Genuine LOW.

### INFO (pass-through verifications)

#### PERF-DIM7-10: TAA shader correctness pass
- `crates/renderer/shaders/taa.comp` unchanged since 2026-05-19. 22 taps/pixel, Halton(2,3) period-16 jitter (#1093), YCoCg variance clamp γ=1.5 (#1108), NaN/Inf guard (#903), motion-vector point sample, bit-15-aware mesh_id disocclusion gate (#904). No new finding.

#### PERF-DIM7-11: Skin compute output buffer lifecycle clean
- `SkinSlot::has_populated_output: bool` first-sight invariant (#1195) — output buffer is never read by BLAS until at least one dispatch has populated it. LRU bump precedes skip gate; quiescent slots not reaped. Pool `sweep` evicts `last_pose_hash` + `pose_dirty` alongside slot reclaim — bounded growth.

#### PERF-DIM7-12: Per-frame bone palette upload single-buffered against MAX_TOTAL_BONES
- `MAX_PENDING_BIND_INVERSE_UPLOADS_PER_FRAME = 227` (was 16 in 2026-05-19 audit) — PERF-DIM7-07 recommendation landed. First-sight 2-frame partial population glitch gone.
- `bind_inverses` is persistent SSBO with per-entity slot pool (M29.6) — single seed-once write per slot, then steady-state zero writes for that entity. Tested in `M29.6` series commits (`5be66790` + hotfix bundle #1191/#1192/#1193).

#### PERF-DIM7-13: M29.3 raster fast-path deferred (per plan), confirmed not landed
- `triangle.vert` still inlines weighted-matrix sum; pre-skinned vertex SSBO is not consumed by raster path. M29.3 (raster fast-path) is deferred per audit/ROADMAP; not a regression.

---

## Prioritized Fix Order

### Quick win (one-line / few-line, zero risk)

1. **PERF-DIM7-14** — Update stale doc-comment at `skin_compute.rs:128-134`. ~3 LOC. **NO RISK** — pure documentation. Ship as a polish commit.

### Optional polish (deferred without prejudice)

2. **PERF-DIM7-08** — Add `debug_assert!(layout_is_general(slot))` in TAA first-sample path; pin the "UNDEFINED is never sampled" invariant. ~5 LOC, LOW risk.

### Defer (real-cost finding, real engineering cost)

3. **PERF-DIM7-05** — TAA history image reuse pool on resize. ~50 LOC, MEDIUM risk (allocator integration). Resize is rare; defer.
4. **PERF-DIM7-09** — Variable-stride bone palette. M29.x milestone, not a hotfix. Out of scope for any single PR.

---

## Notes

- Today's Dim7 sweep landed as a **clean dedup pass**: the four MEDIUM action items from 2026-05-19 (`#1195` / `#1196` / `#1197` / `#1127`) all shipped between 2026-05-21 and 2026-05-24, plus the PERF-DIM7-INSTR prerequisite (#1194) and the LOW PERF-DIM7-07 upload cap bump. That's six fixes in five days against this dimension — a hot week for skinning + TAA.
- "Needs measurement" qualifier from 2026-05-19 is now lifted: `self.gpu_timers` + `dispatches_skipped` counter let `bench-stats --break-down skin` (or `skin.coverage` from `byro-dbg`) quantify the win in a follow-up `--bench-hold` smoke session.
- The only NEW finding (PERF-DIM7-14) is stale prose, not stale code. The instrumentation comment forgot to update when its dependent fix shipped 24 hours later.
- dhat-infra remains the cross-cutting open infrastructure gap (carried since 2026-05-04). Not bearing on Dim 7 today.
- No regression detected against the 2026-05-19 baseline. No new architectural debt introduced.
