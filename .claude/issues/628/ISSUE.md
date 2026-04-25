# FNV-D4-01: cluster_cull FAR=10000 hardcoded, smaller than FNV exterior fog_far (30K-80K)

**Severity:** MEDIUM | renderer, vulkan
**Source:** `docs/audits/AUDIT_FNV_2026-04-24.md`

## Problem
`crates/renderer/shaders/cluster_cull.comp:18` hardcodes `const float FAR = 10000.0`. FNV exterior CLMT routinely sets `fog_far` to 30K-80K. Lights past 10K fall outside every slice's AABB and silently cull from the fragment shader's per-cluster list.

Directional lights are unaffected (flagged "always-affect").

## Audit-prescribed fix
Source FAR (and ideally NEAR) from the same `fog_far` the geometry pass uses — `screen.zw` UBO field already carries fog_near/fog_far.

## SIBLING
Audit cluster_cull.comp for other hardcoded scene-scale constants.

## TESTS
Exterior cell with a light at world-distance 30K should be in the cluster light list.
