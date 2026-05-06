# #821 — REN-D9-NEW-02: Window-portal escape ray skips the V-aligned N_bias hoist

**Severity**: LOW (documentation / consistency only)
**Location**: `crates/renderer/shaders/triangle.frag:1318-1328`
**Source**: `docs/audits/AUDIT_RENDERER_2026-05-04_DIM9.md`
**Created**: 2026-05-04

## Summary

Window-portal escape ray uses raw `-N` instead of the V-aligned
`N_bias` hoist used by every other RT site. Intentional (must start
*outside* the pane; the `windowFacing > 0.1` gate guarantees `-N`
points away from camera here) but undocumented. Copy-paste hazard for
future refactors.

## Fix

Comment block at line 1318 explaining the intentional asymmetry, OR
hoist `N_outward = (windowFacing > 0.1) ? -N : N` once and use it.
No semantic change.

## How to fix

```
/fix-issue 821
```
