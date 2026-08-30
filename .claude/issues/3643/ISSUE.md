# CONC-D1-2026-08-30-01: `MAX_FRAMES_IN_FLIGHT`'s const-assert names only the depth image; four more non-per-FIF resources rely on the same both-slots-wait identity

**Issue**: #3643
**Labels**: documentation, medium, sync, concurrency, doc-rot
**Filed**: 2026-08-30
**Source report**: `docs/audits/AUDIT_CONCURRENCY_2026-08-30.md`

---

Source: `docs/audits/AUDIT_CONCURRENCY_2026-08-30.md` — CONC-D1-2026-08-30-01 (MEDIUM, D1 · Vulkan Queue & AS Sync).

**Location**: `crates/renderer/src/vulkan/sync.rs:8-49` (the #870 block + `const _: () = assert!`), `crates/renderer/src/vulkan/acceleration/blas_skinned.rs:216-231`.

**Documentation-only fix. No impact at HEAD — this is a guard-completeness finding, not a live bug.**

## Description

`sync.rs` correctly identifies that the both-slots `wait_for_fences` is equivalent to device-idle **only** at `MAX_FRAMES_IN_FLIGHT == 2`, and lists two remediations. But it attributes the whole constraint to **one** resource (the shared depth image) and offers option (a) — make *that* image per-FIF — as sufficient.

**It is not.** The same identity is load-bearing for at least four other non-per-FIF resources, none of which is named there and only one of which (`images_in_flight`, `sync.rs:105-110`) carries its own warning:

1. `blas_scratch_buffer`'s **immediate** (deliberately non-deferred, correct today) destroy at `blas_skinned.rs:229-231`;
2. `depth_capture_staging` — destroyed and reallocated *during frame recording* (`depth_capture.rs:137` -> `:240-245`), with a SAFETY comment asserting "no command buffer can still reference `buffer`";
3. `terrain_tile_buffer` — one shared DEVICE_LOCAL buffer overwritten by a blocking staged copy mid-`draw_frame` (`draw.rs:3361-3372` -> `scene_buffer/upload.rs:840`);
4. `screenshot_staging` / `depth_capture_pending_readback`, single-slot host readbacks gated purely on the top-of-frame wait.

Separately, `blas_skinned.rs`'s own SAFETY comment states a **narrower** premise than the one that actually holds: it justifies the immediate free by "this same frame-in-flight slot's previous recording ... has therefore already retired". That argument alone is insufficient — **the other slot's recording also captures the scratch address**. What makes the site safe is the both-slots wait in `draw.rs:1624-1637`, which the comment never mentions. So at 3 slots the comment would still *read* correct while the code became a use-after-free.

## Evidence

```
sync.rs:36-42
// Bumping this constant requires either:
//   (a) making the depth image per-frame-in-flight
//       (`Vec<vk::Image>` indexed by frame_index, mirroring the
//       G-buffer pattern at `gbuffer.rs:52`), OR
//   (b) extending the fence wait to cover all in-flight slots
//       (currently 2; would become MAX_FRAMES_IN_FLIGHT - 1 fences).

sync.rs:46-49
const _: () = assert!(
    MAX_FRAMES_IN_FLIGHT == 2,
    "shared depth image at context/mod.rs:580 requires \
     MAX_FRAMES_IN_FLIGHT == 2; see #870 for the safety contract"
);

blas_skinned.rs:216-231
// SAFETY / not a #1782 sibling: ... this call site
// runs from `record_skinned_blas_refit`, itself called from
// `draw_frame` AFTER that frame's own `wait_for_fences`. Any
// command buffer that could reference the *old* scratch
// buffer's device address (this same frame-in-flight slot's
// previous recording) has therefore already retired. Do NOT
// "fix" this site by copying the deferred-destroy pattern — ...
if let Some(mut old) = self.blas_scratch_buffer.take() {
    old.destroy(device, allocator);
}
```

## Trigger Conditions

Latent — **unreachable at HEAD** (`MAX_FRAMES_IN_FLIGHT == 2`, enforced by the const-assert). Reachable the moment someone follows remediation option **(a)** and bumps the constant to 3+ *without* also doing option (b). At 3 slots, `draw_frame`'s wait covers `in_flight[frame]` and `in_flight[(frame+1) % 3]`; slot `(frame+2) % 3` is still executing. `build_skinned_blas_batched_on_cmd` then immediately `destroy()`s a `blas_scratch_buffer` whose device address that slot's recorded `cmd_build_acceleration_structures` still holds -> **AS build against freed memory**.

## Impact

The const-assert is the project's designated tripwire for this class. Its message offers a remediation that would satisfy the assert-removal **while silently breaking four other invariants** — the "short list read as exhaustive" hazard the same comment block warns about for the depth consumers, applied one level up.

## Related

#870 (the const-assert), #1782 (deferred scratch destroy), #3442 (the source-scan pin that cannot see `(f + 1) % MAX_FRAMES_IN_FLIGHT`), #418 (deferred-destroy tick placement).

## Suggested Fix

Amend the #870 block to state that option (a) alone is **not** sufficient — the both-slots wait is depended on by the immediate scratch free, the depth-capture/screenshot staging destroys, and the terrain-tile buffer — so option (b) (or per-FIF-ing all of them) is mandatory on any bump. Add one line to `blas_skinned.rs:216-228` naming `draw.rs`'s **both**-slots wait as the actual guarantee rather than the slot-local one.

Documentation only; no code change, no barrier change.

## Completeness Checks
- [ ] **SIBLING**: The enumeration is complete — every non-per-FIF resource whose safety rests on the both-slots wait is named, not just the five found here
- [ ] **DROP**: No Vulkan destroy ordering changes as part of this (it is a comment fix); if any does, reverse-order correctness re-checked
- [ ] **TESTS**: Consider a source-shape pin so a future `MAX_FRAMES_IN_FLIGHT` bump has to acknowledge the full list, not just the assert message
