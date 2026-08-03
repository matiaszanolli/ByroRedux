# TD1-082: collect_lights crossed 200 LOC

**Severity**: LOW
**Dimension**: 1 (File/Function/Module Complexity)
**Location**: `byroredux/src/render/lights.rs:106` (`collect_lights`, 179→208 LOC)
**Labels**: low, renderer, tech-debt, bug
**Source**: `docs/audits/AUDIT_TECH-DEBT_2026-08-03.md`

## Description
Grew past the skill's 200-LOC function-size guideline via this window's
light-kind/shadow-flag wiring (#2205, #2250/#2251). Not yet large enough to be a
standalone medium-effort item.

## Suggested Fix
If it grows further, extract the per-light-kind (point/spot/directional) dispatch
arms into named helpers.

## Age / Effort
This window. Effort: trivial (watch-and-wait; no action strictly required yet).
