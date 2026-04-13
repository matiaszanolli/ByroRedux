# #283: C3-01..C3-03 — recreate_on_resize partial-failure leaks

**Severity**: MEDIUM | **Domain**: renderer | **Type**: bug

## Findings
- C3-01: GBuffer `recreate_on_resize` (`gbuffer.rs:283-309`)
- C3-02: SVGF `recreate_on_resize` (`svgf.rs:725-777`)
- C3-03: Composite `recreate_on_resize` (`composite.rs:645-799`)

All three destroy old resources unconditionally, then allocate new ones with `?`.
Partial failure leaks already-allocated resources.

## Fix
Allocate into temporaries and swap on success, or add rollback guard.
