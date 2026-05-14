# Issue #1018

**Title**: REN-D15-NEW-09: Weather cross-fade fog NEAR/FAR locked to source night_factor against target fog table

**Source**: `docs/audits/AUDIT_RENDERER_2026-05-13.md` — REN-D15-NEW-09
**Severity**: MEDIUM
**File**: `byroredux/src/systems/weather.rs:343`

## Issue

Cross-fade fog NEAR/FAR distance is locked to *current* `night_factor` while the colour table is the *target* weather's fog table. During the 8s cross-fade, fog distances are sourced from the old weather but colours from the new — visible as fog colour shifting before distance does.

## Fix

Sample both NEAR/FAR distance and colour from the same blended source. Either: blend NEAR/FAR alongside the colour at the same `t`, or anchor both to either source-or-target consistently.

## Completeness Checks
- [ ] **SIBLING**: Cross-check other WeatherTransitionRes consumers
- [ ] **TESTS**: Cross-fade integration with distinct fog tables

