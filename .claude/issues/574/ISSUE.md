# #574 RT-2: buildOrthoBasis produces NaN tangent when fragment normal is exactly (0,1,0)

**Severity**: MEDIUM  
**Audit**: AUDIT_RENDERER_2026-04-22  
**File**: `crates/renderer/shaders/triangle.frag:250-254`

## Summary

`buildOrthoBasis` threshold `abs(dir.y) < 0.999` fails to catch exactly-upward normals. When `dir=(0,1,0)`, `cross((0,1,0),(0,1,0))=(0,0,0)`, `normalize(0)=NaN`. Common on flat LAND quads. NaN propagates into `rayQueryInitializeEXT` = undefined behavior.

## Fix

Option A: raise threshold to `< 0.9999`.  
Option B (preferred): Frisvad (2012) singularity-free orthonormal basis.
