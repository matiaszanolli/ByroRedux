# CONC-D2-2026-08-30-02: `CausticPipeline::clear_for_skip`'s `TRANSFER_WRITE` is never in the next dispatch's decay-read source scope

**Issue**: #3646
**Labels**: bug, renderer, medium, sync, concurrency
**Filed**: 2026-08-30
**Source report**: `docs/audits/AUDIT_CONCURRENCY_2026-08-30.md`

---

Source: `docs/audits/AUDIT_CONCURRENCY_2026-08-30.md` — CONC-D2-2026-08-30-02 (MEDIUM, D2 · Compute -> AS -> Fragment Chains).

**Location**: `crates/renderer/src/vulkan/caustic.rs:1144-1161` (publish) x `crates/renderer/src/vulkan/caustic.rs:957-972` (the next visit's `pre_decay`).

## Description

`clear_for_skip` deliberately publishes only to `FRAGMENT_SHADER` — its own comment says *"TRANSFER -> FRAGMENT directly (no compute dispatch follows this clear)"*. That is true **within the frame**, but the slot's **next** visit is `dispatch`, whose `pre_decay` barrier names `COMPUTE_SHADER | FRAGMENT_SHADER` / `SHADER_READ | SHADER_WRITE` as the *source* scope.

`TRANSFER` / `TRANSFER_WRITE` appears in **neither** the earlier publish's dst **nor** the later barrier's src, so no dependency chain carries the clear's write to the decay pass's `imageLoad`. The `pre_decay` doc comment enumerates the prior uses it expects ("prior splat compute-write + composite fragment-read") and the `clear_for_skip` path is simply not in that list.

Note this is **not** covered by the both-slots fence wait under the codebase's own stated doctrine — `skinned_blas_refit.rs:568-580` explicitly says *"the host fence-wait is a host-side dependency only and does NOT establish device-side memory ordering for the next submission"*.

## Evidence

```rust
// caustic.rs — clear_for_skip publishes to FRAGMENT only
1144        // TRANSFER → FRAGMENT directly (no compute dispatch follows this
1145        // clear, unlike `dispatch`'s TRANSFER → COMPUTE mid-barrier).
1146        let post_clear_barrier = vk::ImageMemoryBarrier::default()
1147            .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
1148            .dst_access_mask(vk::AccessFlags::SHADER_READ)
1155            vk::PipelineStageFlags::TRANSFER,
1156            vk::PipelineStageFlags::FRAGMENT_SHADER,
```
```rust
// caustic.rs — the next visit's pre_decay src scope omits TRANSFER
957            let pre_decay = vk::ImageMemoryBarrier::default()
958                .src_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE)
959                .dst_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE)
966                vk::PipelineStageFlags::COMPUTE_SHADER | vk::PipelineStageFlags::FRAGMENT_SHADER,
967                vk::PipelineStageFlags::COMPUTE_SHADER,
```
```glsl
// caustic_splat.comp:241-243 — the decay pass reads the accumulator
241  if (pc.decayOnly == 1u) {
242      for (int channel = 0; channel < 3; ++channel) {
243          uint v = imageLoad(causticAccum, ivec3(pixel, channel)).r;
```

## Trigger Conditions

For one frame-in-flight slot `f`: frame N skips the caustic dispatch (no TLAS for `f`, or `caustic_failed` latched) so `caustic_skip_clear_decision` fires `clear_for_skip(f)`; frame N+`MAX_FRAMES_IN_FLIGHT` reaches the slot again with `history_valid == true` (camera parked, scene static) and takes the `if history_valid` decay branch, whose shader `imageLoad`s the accumulator. **Reachable at cell-load settle**: the TLAS is absent for the first frames while the camera has not yet moved.

## Verification Path — REQUIRED BEFORE ANY FIX LANDS

Validation layer, per the project's speculative-Vulkan-fix rule. `BYRO_VALIDATION=1` **release** build; force the window by launching with `--cell <interior with water>` and holding the camera still through load.

Expected concrete signal: **`SYNC-HAZARD-READ-AFTER-WRITE`** on the caustic accumulator `VkImage` at the decay `vkCmdDispatch` (`caustic.rs:981`) with `prior_access = SYNC_COPY_TRANSFER_WRITE` (the `vkCmdClearColorImage` in `clear_for_skip`). Not observable via `cargo test`; the visible artifact (a one-frame stale caustic pool after a load) is too subtle to be a reliable signal on its own.

## Impact

The decay pass can scale the **pre-clear** accumulator contents instead of zeros, so a caustic pool that #2507 intended to clear survives one extra slot visit. Purely cosmetic and self-correcting (the EMA converges), which is why this is MEDIUM and not HIGH — but it becomes load-bearing the moment `MAX_FRAMES_IN_FLIGHT` is raised or the both-slots fence wait is relaxed (the relaxation #653 and `svgf.rs:1256-1268` both anticipate).

## Related

#2507 (the skip-clear this weakens); #653 (the "fence currently serialises this, but the mask must still be right" precedent, already applied in `taa.rs:781` and `svgf.rs:1315`); CONC-D2-2026-08-30-03 (the volumetrics sibling, WAW instead of RAW).

## Suggested Fix

Add `vk::PipelineStageFlags::TRANSFER` / `vk::AccessFlags::TRANSFER_WRITE` to the source scope of `dispatch`'s `pre_decay` and `pre_clear_barrier` (`caustic.rs:957-972`, `:1002-1015`) — or, equivalently and more locally, widen `clear_for_skip`'s `post_clear_barrier` dst to `FRAGMENT_SHADER | COMPUTE_SHADER`. Both are **additive-only** mask widenings; confirm with the sync-val signal before/after.

## Completeness Checks
- [ ] **VALIDATION**: The `SYNC-HAZARD-READ-AFTER-WRITE` signal is observed before the change and absent after it — do not land on reasoning alone
- [ ] **SIBLING**: CONC-D2-2026-08-30-03 (`volumetrics.rs` `record_neutral_frame`) is the same pattern; fix both in one pass
- [ ] **TESTS**: A source-shape pin that a skip-path clear's dst mask is a superset of the next-visit barrier's src mask
