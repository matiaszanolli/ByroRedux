# #257: R-02 — Contact-hardening penumbra scaling inverted

**Severity**: MEDIUM | **Domain**: renderer | **Type**: bug
**Location**: `crates/renderer/shaders/triangle.frag:548-549`
**Source**: `AUDIT_RENDERER_2026-04-12.md`

## Problem
`distRatio` grows with distance, making jitter disk larger for distant fragments. Physically backwards — near fragments should have wider penumbra.

## Fix
Invert the ratio or implement PCSS-style contact hardening using hit distance.
