---
issue: 579
title: "SAFE-20: SAFETY comment coverage gap in post-April renderer modules"
labels: medium, safety, renderer, documentation
state: OPEN
audit: docs/audits/AUDIT_SAFETY_2026-04-23.md § SAFE-20
---

## Summary

98 unsafe blocks across 6 post-April renderer modules have almost no SAFETY comments. Project convention (`.claude/commands/_audit-severity.md`) puts this at MEDIUM.

| File | Unsafe | SAFETY |
|------|-------:|-------:|
| caustic.rs | 19 | 0 |
| composite.rs | 25 | 1 |
| ssao.rs | 10 | 0 |
| taa.rs | 17 | 0 |
| svgf.rs | 18 | 0 |
| gbuffer.rs | 9 | 1 |

## Fix
One-line SAFETY comment per `unsafe { device.X(…) }` block, following the `acceleration.rs` model (22 unsafe / 9 SAFETY covering every device-address query + union init).

## Completeness
- [ ] SAFETY comments name concrete invariants, not boilerplate
- [ ] All 6 modules covered consistently
