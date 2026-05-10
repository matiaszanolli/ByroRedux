---
issue: 0
title: REN-D10-NEW-04: SVGF history-recovery boundary fragile if MAX_FRAMES_IN_FLIGHT changes — add const_assert
labels: renderer, medium, vulkan, safety
---

**Severity**: MEDIUM (defence-in-depth; ping-pong arithmetic invariant)
**Source audit**: docs/audits/AUDIT_RENDERER_2026-05-09.md (Dim 10)

## Location

- `crates/renderer/src/vulkan/svgf.rs` — ping-pong slot index arithmetic

## Why it's a bug

Ping-pong arithmetic in SVGF (and TAA — see related) assumes `MAX_FRAMES_IN_FLIGHT >= 2`. If the constant is ever lowered to 1 (single-frame-in-flight CPU-bound mode), the read-previous / write-current pattern silently aliases to the same slot.

## Fix sketch

Add `const_assert!(MAX_FRAMES_IN_FLIGHT >= 2)` next to the ping-pong arithmetic site. Either:
- Use `static_assertions::const_assert!`, or
- Inline `const _: () = assert!(MAX_FRAMES_IN_FLIGHT >= 2);`

## Completeness Checks

- [ ] **SIBLING**: TAA has the same ping-pong assumption; pin the assert there too.
- [ ] **TESTS**: Compile-time assertion is the test.

🤖 Filed by /audit-publish from docs/audits/AUDIT_RENDERER_2026-05-09.md
