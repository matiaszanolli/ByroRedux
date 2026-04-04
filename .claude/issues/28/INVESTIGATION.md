# Investigation: Issue #28

## Root Cause
triangle.vert line 25:
```glsl
fragNormal = transpose(inverse(mat3(pc.model))) * inNormal;
```

Two problems:
1. **NaN**: `inverse()` divides by determinant. Zero-scale → det=0 → NaN
2. **Redundant**: Transform uses uniform scale (f32), so inverse-transpose
   of the upper 3x3 equals the upper 3x3 itself. No inverse needed.

## Fix
Replace with:
```glsl
vec3 n = mat3(pc.model) * inNormal;
fragNormal = (dot(n, n) > 0.0) ? normalize(n) : vec3(0.0, 1.0, 0.0);
```

This:
- Eliminates the per-vertex inverse (was same result for every vertex anyway)
- Guards against zero-scale by checking length before normalize
- Falls back to up-vector for degenerate normals

## Scope
1 file: triangle.vert. Recompile SPIR-V.
