# REN-D8-2026-09-20-04: a failed combustion-moment drain consumes combustion_moment_dirty before the fallible read/zero/flush — stale moments persist one cycle

- **ID**: REN-D8-2026-09-20-04
- **Labels**: low,renderer,bug
- **Filed from**: docs/audits/AUDIT_RENDERER_2026-09-20.md

**Severity**: LOW · **Dimension**: Volumetrics
**Source**: docs/audits/AUDIT_RENDERER_2026-09-20.md (REN-D8-2026-09-20-04)

**Location**: `crates/renderer/src/vulkan/volumetrics.rs` — `append_combustion_surface_lights` drain path

**Description**
The dirty flag is consumed before the fallible read/zero/flush sequence; on failure the moments stay un-zeroed for one extra cycle (self-healing on the next successful drain). Unreachable today (the read does not fail on live content) — a one-line latch-restore hardening.

**Evidence**
Audit D8, 2026-09-20.

**Impact**
One frame of stale combustion light if the read ever fails.

**Suggested Fix**
Restore the latch on the error path (or consume the flag only after the flush succeeds).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
