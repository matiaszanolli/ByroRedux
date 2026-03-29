---
description: "Audit compatibility gaps between Gamebryo 2.3 and Redux — what's mapped, what's missing"
---

# Legacy Compatibility Audit

Read `_audit-common.md` and `_audit-severity.md` for shared protocol.

## Purpose

Compare the Gamebryo 2.3 architecture against Redux's current implementation.
Identify gaps that block NIF loading, animation playback, or content rendering.

## Dimensions

### 1. Scene Graph Decomposition
- Read `docs/legacy/api-deep-dive.md` mapping table
- For each NiAVObject field: does the Redux component exist?
- Missing components needed for NIF import (Parent, Children, WorldTransform, WorldBound, etc.)

### 2. NIF Format Readiness
- Is there a NIF parser crate? What object types can it parse?
- Version coverage: which NIF versions are supported?
- Link resolution: can cross-references between objects be resolved?
- What Gamebryo object types are needed minimum (NiNode, NiTriShape, NiTriShapeData)?

### 3. Transform Compatibility
- Gamebryo: NiTransform = Matrix3 rotation + Point3 translation + float scale
- Redux: Transform = Quat rotation + Vec3 translation + f32 scale
- Is there a conversion function (Matrix3 → Quat) for NIF import?
- Is world transform propagation implemented (local * parent = world)?

### 4. Property → Material Mapping
- Gamebryo has 12 NiProperty types (alpha, texturing, material, zbuffer, etc.)
- Which map to Vulkan pipeline state vs per-object components?
- Is there a Material component in Redux?

### 5. Animation Readiness
- Gamebryo: NiTimeController → NiInterpolator → keyframes
- Redux: any keyframe data structures? Interpolation system?
- KF/KFM file format: parser exists?

### 6. String Interning Alignment
- Gamebryo: NiFixedString with GlobalStringTable
- Redux: StringPool + FixedString
- Are they semantically equivalent? Any gaps?

## Process

1. Read Redux component implementations in `crates/core/src/ecs/components/`
2. Cross-reference against Gamebryo headers in the legacy source
3. For each gap: classify as CRITICAL (blocks NIF loading), HIGH (blocks rendering), MEDIUM (blocks full fidelity), LOW (cosmetic)
4. Save report to `docs/audits/AUDIT_LEGACY_COMPAT_<TODAY>.md`
