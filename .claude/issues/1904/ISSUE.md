# SAFE-D4-01: ~134 renderer FFI unsafe {} blocks carry no SAFETY comment (batched)

**Issue**: #1904 · **Severity**: MEDIUM · **Labels**: medium, renderer, safety, bug
**Dimension**: Unsafe-Block Discipline (batched; _audit-severity Special Rule "unsafe block without safety comment = MEDIUM")
**Filed from**: docs/audits/AUDIT_SAFETY_2026-07-06.md (nif-deep suite)
**Location**: crates/renderer/src/vulkan/ (batched). Representative: buffer.rs:734, water.rs:236,273,308,330,653,660,
instance.rs:89,111, context/mod.rs:1677,1779,1990,1996,2041,2088.

## Description
Of 607 unsafe tokens in the renderer, 70 are unsafe fn decls (acceptable) and ~134 are unsafe {} blocks
with no SAFETY note in ±5 lines. Spot-checks: almost all single ash object-creation FFI calls
(create_descriptor_set_layout, create_fence, create_graphics_pipelines) — low-hazard but each trips the
project's own "unsafe-without-comment = MEDIUM" rule. NOTE: the raw 596-vs-403 gap overstates it — many
SAFETY comments sit on the first line INSIDE the block (missed by preceding-line scan); true undocumented
count ~134, not ~190.

## Impact
Documentation/hardening only — no unsound invariant found. Uncommented blocks give a future edit
(e.g. from_raw_parts on mapped memory) no stated invariant to check against.

## Suggested Fix
One-line SAFETY: per block tying it to the standard ash precondition (device live, handles created by
this device, not in use by an in-flight cmd buffer). File-by-file. Consider clippy::undocumented_unsafe_blocks
(allow at crate root, deny per-file as cleaned) to prevent regrowth.

**Related**: _audit-severity Special Rules. Distinct from #1861 (a specific fence/cmd-buffer leak).
