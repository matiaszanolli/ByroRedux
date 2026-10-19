# #6063 — SAFE-2026-10-19-02 (snapshot as filed)

Labels: low,safety,renderer,vulkan,bug
Report: docs/audits/AUDIT_SAFETY_2026-10-19.md

**Severity**: LOW — trigger is `vkWaitForFences` failing with an out-of-memory error after a successful submit; `ERROR_DEVICE_LOST` is spec-safe. Same class as closed #5947 (the #4891 policy, `OneTimeCommandError::may_be_in_flight`).
**Dimension**: Dim 3 (drop ordering) / Dim 5 (VUID class #4891 / #5201 / #5270)
**Source**: `docs/audits/AUDIT_SAFETY_2026-10-19.md` (SAFE-2026-10-19-02)

## Location
- `crates/renderer/src/vulkan/shadow_filter.rs:91-96` (`new`): any `new_inner` error, including the one from `initialize_layouts` (→ `with_one_time_commands`), runs `partial.destroy(...)` unconditionally; the SAFETY comment says "nothing created so far has been submitted", false for the `MaybeInFlight` arm.
- `crates/renderer/src/vulkan/context/resize.rs:717-740` (resize recreate): on `Err` it calls `filter.destroy(...)` under "the resize path waited both frames in flight", which covers frame work but not the pending one-time layout submit.

Pass first landed in `ee0ba7069` (2026-10-15). #5947 fixed this exact pattern for volumetrics only (`context/init.rs`, `context/resize.rs` now consult `may_be_in_flight` and `mem::forget`).

## Description
The one-time command buffer transitions the filter's six images (ratio A, ratio B and correction, per frame in flight) UNDEFINED → GENERAL. If the submit succeeded and the fence wait failed, the transition may still be executing against images that `destroy` frees (VUID-vkDestroyImage-image-01000 territory). TAA/SVGF/caustic avoid it by logging and continuing on a failed `initialize_layouts` (`taa.rs:818`, `svgf.rs:1431`, `caustic.rs:1190`) so they never free in that arm; the shadow filter is the first that frees.

## Evidence
No `may_be_in_flight` reference in `shadow_filter.rs`; `ShadowFilterPipeline::new` and `recreate_on_resize` both propagate a bare `?` from `initialize_layouts` into a destroying caller. `with_one_time_commands` leaks its own fence/cmd buffer on that arm (`texture.rs:933-960`), so the caller is the only party that would free live-referenced memory. Window is real but needs a fault injection.

## Suggested Fix
Mirror `init.rs:1100-1112`: `std::mem::forget` the pipeline (or its image set) when `OneTimeCommandError::may_be_in_flight(&e)`, destroy otherwise, in both `new` and the resize arm; pin with the same source-scan style used for `cloud_noise.rs`.

## Completeness Checks
- [ ] **UNSAFE**: Updated SAFETY comments state the true invariant per arm
- [ ] **SIBLING**: Other post passes that destroy on `initialize_layouts` failure re-checked
- [ ] **DROP**: `context/teardown.rs` ordering (shadow filter before reservoir buffers and allocator unwrap) remains correct
- [ ] **TESTS**: A source-scan/regression test pins the `may_be_in_flight` handling in both sites
