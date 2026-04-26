# Coordinate System & Transform Pipeline

This document describes how ByroRedux converts coordinates from Gamebryo's
Z-up convention to the renderer's Y-up convention, and how REFR (placed
reference) transforms from ESM plugin files are composed with NIF scene
graph transforms.

## Coordinate Systems

### Gamebryo / NIF (Z-up, right-handed, **clockwise-positive** rotations)

```
    Z (up)
    |
    |
    +---- X (right)
   /
  Y (forward)
```

- **X** = right
- **Y** = forward (into the world)
- **Z** = up
- Right-handed: X x Y = Z
- **Rotation convention: clockwise-positive** (see below)

### ByroRedux / Vulkan (Y-up, right-handed, counter-clockwise-positive)

```
    Y (up)
    |
    |
    +---- X (right)
   /
  Z (backward / out of screen)
```

- **X** = right
- **Y** = up
- **Z** = backward (out of screen); **-Z** = forward
- Right-handed: X x Y = Z

### Conversion

The change-of-basis matrix C maps `(x, y, z)_zup -> (x, z, -y)_yup`:

```
C = | 1   0   0 |
    | 0   0   1 |     det(C) = +1  (proper rotation, preserves winding)
    | 0  -1   0 |
```

This is applied to:
- **Vertex positions**: `[v.x, v.z, -v.y]`
- **Vertex normals**: `[n.x, n.z, -n.y]`
- **Translations**: `[t.x, t.z, -t.y]`
- **Rotation matrices**: `R_yup = C * R_zup * C^T` (conjugation)

## Gamebryo's Clockwise-Positive Rotation Convention

Gamebryo uses a **non-standard clockwise-positive** rotation convention,
confirmed in the Gamebryo 2.3 source (`NiMatrix3.h` lines 17-35). Its
elementary rotation matrices are:

```
Rx_cw(t) = | 1    0      0   |     Ry_cw(t) = | cos  0  -sin |     Rz_cw(t) = |  cos  sin  0 |
           | 0   cos    sin  |                 |  0   1    0  |                 | -sin  cos  0 |
           | 0  -sin    cos  |                 | sin  0   cos |                 |   0    0   1 |
```

These are the **transpose** of the standard counter-clockwise matrices.
Equivalently, `Rx_cw(t) = Rx_standard(-t)`.

### Impact on NIF rotation matrices

For **matrix operations** (compose, matrix * point), the CW convention
is transparent. The NIF stores the rotation matrix directly, and
`M * point` produces the same physical result regardless of how the
angle is labeled. Our `compose_transforms` and `zup_matrix_to_yup_quat`
work directly on NIF matrices without transposing.

### Impact on Euler angles (REFR DATA)

For **Euler angles** stored in ESM/ESP REFR DATA subrecords, the
convention matters because the same float value represents opposite
rotations in CW vs CCW conventions:

```
Bethesda CW:    R = Rz_cw(rz)  * Ry_cw(ry)  * Rx_cw(rx)
Standard CCW:   R = Rz_std(-rz) * Ry_std(-ry) * Rx_std(-rx)
```

When converting REFR Euler angles to a Y-up quaternion (using glam's
standard CCW convention), all angles must be **negated**, then the
Z-up to Y-up conjugation applied:

```
R_yup = Ry(-rz) * Rz(ry) * Rx(-rx)
```

In code (`cell_loader.rs`):
```rust
fn euler_zup_to_quat_yup(rx: f32, ry: f32, rz: f32) -> Quat {
    Quat::from_rotation_y(-rz) * Quat::from_rotation_z(ry) * Quat::from_rotation_x(-rx)
}
```

The conjugation derivation:
- `C * Rx(-rx) * C^T = Rx(-rx)` — X axis is unchanged
- `C * Ry(-ry) * C^T = Rz(ry)` — Y maps to -Z, double negate
- `C * Rz(-rz) * C^T = Ry(-rz)` — Z maps to Y

## Transform Pipeline

### 1. NIF Import (`crates/nif/src/import.rs`)

The NIF scene graph is walked recursively. At each node, transforms are
composed in **Z-up space** using Gamebryo's matrix math:

```
world_transform = parent.rot * (parent.scale * child.trans) + parent.trans
world_rotation  = parent.rot * child.rot
world_scale     = parent.scale * child.scale
```

For degenerate parent rotation matrices (det far from 1.0, e.g.
BSFadeNode roots with garbage data), SVD repair is used:
- **Max singular value < 0.01**: matrix is garbage, use identity
- **Max singular value >= 0.01**: extract nearest valid rotation via `U * Vt`

The composed world transform and local vertex data are then converted
to Y-up independently:
- Vertex positions/normals: `(x, y, z) -> (x, z, -y)`
- Translation: `(x, y, z) -> (x, z, -y)`
- Rotation matrix: `C * M * C^T` -> quaternion via SVD + nalgebra

### 2. Cell Loading (`byroredux/src/cell_loader.rs`)

Each REFR (placed reference) has a Z-up position and Euler rotation.
These are converted to Y-up:

```rust
let ref_pos = Vec3::new(position[0], position[2], -position[1]);
let ref_rot = euler_zup_to_quat_yup(rotation[0], rotation[1], rotation[2]);
```

Then composed with the NIF-internal Y-up transforms:

```rust
let final_pos = ref_rot * (ref_scale * nif_pos) + ref_pos;
let final_rot = ref_rot * nif_quat;
let final_scale = ref_scale * mesh.scale;
```

### 3. Rendering (`crates/renderer/`)

The ECS `Transform` component stores Y-up translation, rotation (quat),
and uniform scale. The model matrix is built via:

```rust
Mat4::from_scale_rotation_translation(Vec3::splat(scale), rotation, translation)
```

The vertex shader applies:
```glsl
gl_Position = viewProj * model * vec4(inPosition, 1.0);
```

### 4. Projection

The camera uses `Mat4::perspective_rh` (Z in [0, 1], Vulkan convention)
with a Y-flip for Vulkan's inverted Y axis:

```rust
let mut proj = Mat4::perspective_rh(fov_y, aspect, near, far);
proj.col_mut(1).y *= -1.0;
```

The Y-flip reverses apparent triangle winding in clip space: CW
triangles from NIF data appear CCW after projection, matching the
pipeline's `front_face: COUNTER_CLOCKWISE` setting.

## Triangle Winding

NIF triangle data uses **CW front face** (D3D convention). The full
winding chain:

1. NIF stores CW triangles
2. Z-up -> Y-up vertex conversion: det(C) = +1, winding preserved (still CW)
3. Model/view transforms: right-handed, det = +1, winding preserved (still CW)
4. Projection Y-flip: reverses apparent winding (CW appears CCW)
5. Pipeline front face: CCW = front -> NIF faces are front-facing

Backface culling is enabled (`CullModeFlags::BACK` with
`front_face = CCW`); the empirical winding verification has landed.
Two-sided meshes (from `NiStencilProperty` on FO3/FNV or
`SF_DOUBLE_SIDED` on Skyrim+/FO4) use a dedicated `CullModeFlags::NONE`
pipeline variant — the cull mode is keyed into the per-`(src, dst,
two_sided)` blend pipeline cache.

## NiTriStrips Winding

Triangle strips alternate winding per-triangle to maintain consistent
front face direction. Our `to_triangles()` conversion:

```rust
for i in 2..strip.len() {
    let (a, b, c) = if i % 2 == 0 {
        (strip[i-2], strip[i-1], strip[i])   // even: standard order
    } else {
        (strip[i-2], strip[i], strip[i-1])   // odd: swap last two
    };
}
```

Each strip resets the alternation independently (i starts at 2 per strip).

## UV Coordinates

NIF UV coordinates use top-left origin (U right, V down), matching
Vulkan's texture coordinate convention. No conversion needed.

## Summary Table

| Data                | Source Space      | Conversion           | Result Space    |
|---------------------|-------------------|----------------------|-----------------|
| Vertex position     | NIF local, Z-up   | `(x, z, -y)`        | Y-up local      |
| Vertex normal       | NIF local, Z-up   | `(x, z, -y)`        | Y-up local      |
| NIF translation     | NIF world, Z-up   | `(x, z, -y)`        | Y-up world      |
| NIF rotation matrix | NIF world, Z-up   | `C*M*C^T` -> quat   | Y-up world      |
| REFR position       | ESM world, Z-up   | `(x, z, -y)`        | Y-up world      |
| REFR Euler angles   | ESM world, Z-up CW| Negate + conjugate   | Y-up quat (CCW) |
| UV coordinates      | NIF, top-left      | None                 | Vulkan, top-left|
| Triangle indices    | NIF, CW front      | None                 | CW -> CCW via Y-flip |
