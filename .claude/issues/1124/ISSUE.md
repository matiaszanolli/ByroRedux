# #1124 — REN-D12-NEW-01: TAA frames_since_creation shared across FIF slots

**Severity:** LOW (latent — currently benign at MFIF=2 with shared resets)
**Domain:** renderer (TAA pipeline)
**Status:** OPEN at HEAD `1608e6a2`

## Summary
SVGF migrated `frames_since_creation` to per-FIF `[u32; MAX_FRAMES_IN_FLIGHT]`
via #964 (REN-D10-NEW-07). TAA still uses a shared `u32`. The issue body
explicitly states the latent risk is **doc parity**, not behaviour: both TAA
history slots always reset together via `recreate_on_resize` /
`signal_history_reset`, and `should_force_history_reset` returns true while
`counter < MAX_FRAMES_IN_FLIGHT` — so at MFIF=2 both slots get their bootstrap
write. Picking option (b) per the issue's own recommendation: add a doc-comment
explaining why the shared counter is correct here.

## Plan
- Append rationale comment at the `frames_since_creation: u32` declaration
  (taa.rs:103) cross-referencing #964 and the TAA-specific reset chain.
- No code change.
