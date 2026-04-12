# #265: LC-05 — AnimationStack clones channel Vecs per frame

**Severity**: MEDIUM | **Domain**: animation, performance | **Type**: enhancement
**Location**: `byroredux/src/systems.rs:454-456`

## Problem
float/color/bool channels cloned from clip every frame to work around lock ordering. Heap allocation per animated entity per frame.

## Fix
Cache clip handle + time, drop stack lock, access registry directly without cloning.
