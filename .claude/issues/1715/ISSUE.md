# SPT-NEW-04: billboard.rs BsRotateAboutUp comment claims "local Z axis" rotation but the code locks world Y

**Issue**: #1715
**Source audit**: `docs/audits/AUDIT_SPEEDTREE_2026-06-23.md`
**Severity**: LOW · **Labels**: low, tech-debt, documentation
**Dimension**: Placeholder Fallback (doc nit)
**Location**: `byroredux/src/systems/billboard.rs:124-136`

## Description

The `BsRotateAboutUp` arm comment says "Rotate only around the billboard's local Z axis (stays in its local X-Y plane). We don't have the local frame here, so fall back to the world-up lock." The code then sets `to_cam.y = 0.0` and uses the XZ-projected to-camera vector — it locks world Y, exactly like the `RotateAboutUp` arm above. The "local Z axis" phrasing is inaccurate and can send a contributor chasing a non-existent local-frame requirement.

## Evidence

`billboard.rs:124-136` — the `BsRotateAboutUp` branch is byte-for-byte the same logic as the `RotateAboutUp` branch (`:112-123`), both zeroing `to_cam.y`. Confirmed in current tree.

## Impact

Comment-only. Rotation behaviour is correct for the placeholder. No visual defect.

## Suggested Fix

Reword to "approximated as a world-up (Y) yaw lock; `BsRotateAboutUp`'s true local-Z rotation needs the node's local frame, which we don't carry here — visually identical for foliage imposters whose vertical axis is world +Y."
