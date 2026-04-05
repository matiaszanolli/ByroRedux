# Investigation: Issue #54

## Root Cause
zup_matrix_to_yup_quat (import.rs:789) always performs nalgebra SVD
(~O(n³) for 3x3) on every rotation matrix. is_degenerate_rotation()
exists (line 691) but is never called in this path.

99% of NIF rotation matrices are valid (det ≈ 1.0) and can use direct
quaternion extraction via Rotation3::from_matrix_unchecked, which is
essentially free (just reinterpret the matrix data).

## Fix
1. Apply Z-up → Y-up axis swap
2. Check determinant: if |det - 1.0| < 0.1, use direct extraction
3. Only fall back to SVD for degenerate matrices

## Scope
1 file: import.rs, 1 function changed.
