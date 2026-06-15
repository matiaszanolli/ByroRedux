# TD3-001: Stale GpuCamera size (304 B) in context/mod.rs doc comment

_Filed as #1623 from `docs/audits/AUDIT_TECH_DEBT_2026-06-14.md`. Immutable snapshot as-filed; GitHub is authoritative for live state._

**Severity**: MEDIUM · **Dimension**: Stale Documentation · **Effort**: trivial
**Source audit**: `docs/audits/AUDIT_TECH_DEBT_2026-06-14.md` (TD3-001)
**Status**: NEW (same doc-rot class as CLOSED #1526 / #1321, recurring at a new site)

## Description
The `sun_angular_radius` doc comment in `crates/renderer/src/vulkan/context/mod.rs:678` says the change "doesn't touch GpuCamera's **304 B** layout." `GpuCamera` is now **336 B** — authoritative pin `gpu_camera_is_336_bytes` (`scene_buffer/gpu_instance_layout_tests.rs`). The struct grew 304 → 320 (DOF) → 336 (`render_origin`, #1492).

## Evidence
`context/mod.rs:678` — `/// change doesn't touch GpuCamera's 304 B layout. See #1023 /` — vs `gpu_types.rs` `GpuCamera (336 bytes…)` and the asserting pin `size_of::<GpuCamera>() == 336`.

## Impact
A reader or next auditor cross-checking GpuCamera size against this site reads a contradicting value — exactly the trap #1526 / #1321 fixed elsewhere, resurfaced. Lockstep-drift bait.

## Suggested Fix
Change "304 B" → "336 B", or reword to "reuses the existing `sky_tint.w` slot, no new field" to avoid pinning a size that drifts.

## Related
#1526, #1321 (both CLOSED — same doc-rot class). #1565 (OPEN — sibling GpuCamera 320→336 doc rot in `shader-pipeline.md` / `memory-budget.md`; this is a distinct third site in source).

## Completeness Checks
- [ ] **SIBLING**: No other doc comment pins a stale GpuCamera byte size (cross-check #1565's two doc sites stay in sync)
- [ ] **TESTS**: `gpu_camera_is_336_bytes` remains the single source of truth for the size; the comment no longer asserts a competing number
