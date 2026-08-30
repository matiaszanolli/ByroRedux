# CONC-D1-2026-08-30-03: `FrameSync::images_in_flight`'s deadlock rationale describes the pre-#952 `reset_fences` position; five line citations stale by 1,500-3,600 lines

**Issue**: #3645
**Labels**: documentation, low, sync, concurrency, doc-rot
**Filed**: 2026-08-30
**Source report**: `docs/audits/AUDIT_CONCURRENCY_2026-08-30.md`

---

Source: `docs/audits/AUDIT_CONCURRENCY_2026-08-30.md` — CONC-D1-2026-08-30-03 (LOW, D1 · Vulkan Queue & AS Sync). Doc rot on a sync contract.

**Location**: `crates/renderer/src/vulkan/sync.rs:9`, `:28`, `:47`, `:93-110`.

## Description

`FrameSync::images_in_flight`'s doc explains why the `image_fence != in_flight[frame]` aliasing guard exists:

> "Reusing the slot's own fence would block on an UNSIGNALED handle (it's reset at draw.rs:191) and deadlock."

**#952 moved `reset_fences` to immediately before `queue_submit`** (`draw.rs:3801-3812`), roughly 2,060 lines *after* the guard at `draw.rs:1745-1746`. At the guard, `in_flight[frame]` is still SIGNALED (it was waited on at `:1624-1637` and has not been reset), so waiting on it would return immediately — **the guard is a fast-path skip, not a deadlock preventer**. The doc's stated hazard no longer exists.

The same block's five line citations are all stale by ~1,500-3,600 lines:

| Doc citation | Actual site |
|---|---|
| `context/mod.rs:580-582` (shared depth image) | `context/mod.rs:1460-1461` — line 580 is inside `DrawCommand::material_hash` |
| `draw.rs:108-120` / `:144-156` (both-slots wait) | `draw.rs:1624-1637` |
| `draw.rs:179-186` / `:180` (image-fence read) | `draw.rs:1745-1746` |

## Evidence

```
sync.rs:95-106
/// by the time `draw_frame` next reads it at `context/draw.rs:179-186`.
/// This is upheld upstream by the *both-slots* `wait_for_fences` at
/// `context/draw.rs:144-156`, ...
/// The aliasing guard `image_fence != in_flight[frame]` at draw.rs:180
/// then prevents waiting on the just-reset fence belonging to the
/// current frame slot. Reusing the slot's own fence would block on
/// an UNSIGNALED handle (it's reset at draw.rs:191) and deadlock.
```
```
sync.rs:47   "shared depth image at context/mod.rs:580 requires \
context/mod.rs:577   pub fn material_hash(&self) -> u64 {     <-- what is actually at :580
context/mod.rs:1461  depth_image: vk::Image,                  <-- the real site
```

Verification: `grep -n "reset_fences" context/draw.rs` -> `1763` (a comment recording the move), `3801`, `3811`. There is no `reset_fences` near the image-fence guard.

## Impact

Two effects, no runtime behaviour change:

1. **The const-assert's failure message — the tripwire a future `MAX_FRAMES_IN_FLIGHT` bump lands on — points at the wrong file location for the resource it names.**
2. The aliasing-guard rationale, if believed, would let someone conclude the guard is what makes an early `reset_fences` safe — the exact inversion of what #952 established.

## Related

#952 (REN-D1-NEW-04), #953 (REN-D1-NEW-05), #870, #282, #2794 (the same stale-line-number class already fixed once in `deferred_destroy.rs`).

## Suggested Fix

Replace the five hard line citations with **symbol names** (`draw_frame`'s both-slots `wait_for_fences`, `VulkanContext::depth_image`, `draw_frame`'s pre-submit `reset_fences`) — the `deferred_destroy.rs:38-46` pattern after #2794 — and restate the guard's purpose as "skip a redundant wait on a fence this frame already waited on", **not** "prevent a deadlock on an unsignaled fence".

## Completeness Checks
- [ ] **SIBLING**: Same symbol-not-line-number treatment applied to the rest of `sync.rs`'s citations, and to CONC-D1-2026-08-30-01's #870 block if fixed in the same pass
- [ ] **TESTS**: `_audit-validate.sh` cannot catch stale *line numbers*, only stale paths — note whether a pin is warranted
