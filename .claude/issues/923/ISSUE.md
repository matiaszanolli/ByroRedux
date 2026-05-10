---
issue: 0
title: REN-D15-NEW-01: M40 worldspace cross-fade is dead code — apply_worldspace_weather has no Phase 2 caller
labels: renderer, medium
---

**Severity**: MEDIUM (feature wired but never invoked)
**Source audit**: docs/audits/AUDIT_RENDERER_2026-05-09.md (Dim 15)

## Location

- `byroredux/src/scene.rs:204-396` — `apply_worldspace_weather` definition + cross-fade machinery
- M40 streaming entry point — missing caller

## Why it's a bug

The M40 worldspace cross-fade machinery is correctly implemented in `apply_worldspace_weather`, but the function is invoked **once at bootstrap** and never again. There is no Phase 2 caller wired into the M40 streaming path.

Result: when streaming across worldspaces (exterior border crossings), the weather palette stays frozen at whichever worldspace was loaded first.

## Fix sketch

Wire `apply_worldspace_weather` into the M40 streaming entry point — every worldspace transition should kick the cross-fade with the new worldspace's weather data + a transition duration (e.g. 8 s, matching `WeatherTransitionRes`).

## Completeness Checks

- [ ] **SIBLING**: Verify single-worldspace bootstrap call is unchanged.
- [ ] **TESTS**: Manual: load FNV exterior, walk across a worldspace boundary, verify weather palette cross-fades smoothly.

🤖 Filed by /audit-publish from docs/audits/AUDIT_RENDERER_2026-05-09.md
