# CONC-D1-2026-09-21-02: `draw_frame_guards_on_empty_framebuffers_before_acquire` is vacuous: its needles match only the test's own literals

**Labels**: low, sync, renderer, vulkan, test-gap, bug

Filed via /audit-publish from docs/audits/AUDIT_CONCURRENCY_2026-09-21.md.

**Severity**: LOW (test gap; the property holds today) · **Dimension**: 1 — Vulkan Queue & AS Sync (acquire discipline, `image_available` signal-pending)
**Location**: `crates/renderer/src/vulkan/context/draw.rs:3017-3056`, `mod framebuffers_empty_guard_tests` / `fn draw_frame_guards_on_empty_framebuffers_before_acquire`

**Status**: NEW (the test has been vacuous since 7463204eb, #3282, 2026-09-02)
**Verified against**: HEAD `f97775ca8`.

## Description

- The test asserts that `draw_frame`'s empty-framebuffers guard precedes `.wait_for_fences(` and `.acquire_next_image(` in `draw.rs`. Both calls moved to `context/sync_and_acquire_frame.rs` (`:65` and `:150`) in 7463204eb (#3282). Since then both `find`s match the test's own literals (`draw.rs:3036`, `:3039`).
- So `guard_pos < wait_pos` always holds. Deleting the production guard also passes: the guard needle then matches the test's own literal at `:3025`, which still comes before `:3036`.
- #3991 repaired the identical self-match in the sibling test `skin_dispatch_ran_is_reset_before_both_early_return_guards` (its comment says so) but missed this one.

## Evidence

- `grep -n '\.wait_for_fences(\|\.acquire_next_image(' crates/renderer/src/vulkan/context/draw.rs` returns only lines 3036 and 3039, both inside this test.
- Found at publish time: three sibling tests in this file (`:3148`, `:3169`, `:3238`) use the same guard needle, `find("if self.swapchain.framebuffers.is_empty() {")`. Their ordering assertions are live while the production guard (`draw.rs:1797`) exists. Their `.expect("draw_frame must guard on empty framebuffers (#1211)")` can never fire, though: if the guard is deleted, the needle falls through to this test's literal at `:3025`.

## Impact

The #1211 contract has no live pin. That contract is to skip the frame *before* acquiring; otherwise `image_available[frame]` is left signal-pending, which trips `VUID-vkAcquireNextImageKHR-semaphore-01779` on the next acquire. The property itself holds today: the guard at `draw.rs:1797` precedes the `self.sync_and_acquire_frame(&mut t)` call at `:1812`.

## Related

#1211 (the contract), #3282 (the split that stranded the needles), #3991 (the sibling repair), #3442 (the compose-needles-at-runtime technique).

## Suggested Fix

- Anchor on `self.sync_and_acquire_frame(&mut t)` in `draw.rs`: the guard must precede it.
- Assert that the wait and the acquire live in `sync_and_acquire_frame.rs`.
- Compose every needle at runtime so that none can match the test's own source.

Source: docs/audits/AUDIT_CONCURRENCY_2026-09-21.md (CONC-D1-2026-09-21-02)

## Completeness Checks
- [ ] **SIBLING**: the three other `framebuffers.is_empty()` needles (`draw.rs:3148`, `:3169`, `:3238`) and every other `include_str!("draw.rs")` test checked for self-matching literals
- [ ] **TESTS**: mutation check: deleting the production guard, or moving it after `sync_and_acquire_frame`, makes the repaired test fail
