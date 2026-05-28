# Coordinate System & Transform Pipeline

This document describes how ByroRedux converts coordinates from Gamebryo's
Z-up convention to the renderer's Y-up convention, and how REFR (placed
reference) transforms from ESM plugin files are composed with NIF scene
graph transforms.

> **Reconciliation note (2026-05-28).** Since the prior pass (2026-04-25)
> the Z-up→Y-up helpers were consolidated into a single source of truth
> (`#1044` / TD3-002/003/004 — see [Consolidation](#consolidation-1044--td3-00x)
> below), the REFR Euler→quaternion convention was corrected from an
> XYZ-product to a ZYX-product (`#`, 2026-05-26), and several file paths
> moved when `import.rs` / `cell_loader.rs` were split into submodule
> directories (Sessions 34/35). All paths, formulas, and constants in this
> doc are verified against the current tree.

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
- Right-handed: X × Y = Z
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
- Right-handed: X × Y = Z

### Conversion

The change-of-basis matrix C maps `(x, y, z)_zup → (x, z, -y)_yup`:

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

## Where the conversion lives

As of `#1044` (TD3-002/003/004), there is **one canonical home** for the
array-form axis swap:

- [`crates/core/src/math/coord.rs`](../../crates/core/src/math/coord.rs) —
  the single source of truth. Exposes the primitive helpers every Bethesda
  import boundary uses:
  - `zup_to_yup_pos([f32; 3]) -> [f32; 3]` — the `(x, z, -y)` swap.
  - `zup_to_yup_quat_wxyz([f32; 4]) -> [f32; 4]` — Gamebryo `(w,x,y,z)`
    Z-up quaternion → glam `(x,y,z,w)` Y-up quaternion, normalised.
  - `euler_zup_to_quat_yup(rx, ry, rz) -> Quat` — REFR / XCLL Euler triple
    → Y-up quaternion (see [Euler angles](#impact-on-euler-angles-refr-data)).
  - `normalize_quat([f32; 4]) -> [f32; 4]` — the `#333` unit-length guard,
    public so the NIF-matrix path can share it.
  - `EXTERIOR_CELL_UNITS: f32 = 4096.0` and
    `cell_grid_to_world_yup(gx, gy) -> Vec3` — exterior cell grid origin
    (see [Exterior cell grid](#exterior-cell-grid)).
- [`crates/nif/src/import/coord.rs`](../../crates/nif/src/import/coord.rs) —
  the **NIF flavour**. Wraps the array primitives with the NIF-internal
  types `NiPoint3` / `NiMatrix3` (`zup_point_to_yup`, `zup_matrix_to_yup_quat`).
  The matrix→quaternion path (Shepperd extraction + nalgebra SVD fallback)
  stays here because it depends on NIF types and isn't shared with any
  other consumer.

### Consolidation (`#1044` / TD3-00x)

Pre-`#1044` the same `(x, z, -y)` transform was duplicated across at least
five sites — `nif::import::coord`, `nif::anim::coord`,
`byroredux::cell_loader::euler`, and the SpeedTree importer — and the
matrix-flavoured Shepperd path had the `#333` normalise-after-extract fix
that its array-form sibling never picked up. All of those now route through
`crates/core/src/math/coord.rs`, so the `#333` unit-quaternion invariant
holds uniformly.

## Gamebryo's Clockwise-Positive Rotation Convention

Gamebryo uses a **non-standard clockwise-positive** rotation convention,
confirmed in the Gamebryo 2.3 source. Its elementary rotation matrices
are the **transpose** of the standard counter-clockwise matrices —
equivalently, `Rx_cw(t) = Rx_standard(-t)`.

```
Rx_cw(t) = | 1    0      0   |     Ry_cw(t) = | cos  0  -sin |     Rz_cw(t) = |  cos  sin  0 |
           | 0   cos    sin  |                 |  0   1    0  |                 | -sin  cos  0 |
           | 0  -sin    cos  |                 | sin  0   cos |                 |   0    0   1 |
```

### Impact on NIF rotation matrices

For **matrix operations** (compose, matrix × point), the CW convention
is transparent. The NIF stores the rotation matrix directly, and
`M * point` produces the same physical result regardless of how the
angle is labeled. `compose_transforms` and `zup_matrix_to_yup_quat` work
directly on NIF matrices without transposing — see the doc comment on
[`zup_matrix_to_yup_quat`](../../crates/nif/src/import/coord.rs).

### Impact on Euler angles (REFR DATA)

For **Euler angles** stored in ESM/ESP REFR DATA subrecords, the
convention matters because the same float value represents opposite
rotations in CW vs CCW conventions. Bethesda applies the CW rotations as a
**ZYX matrix product** — X applied first in object-local axes, Z applied
last when reading the product left-to-right (matching the Creation Kit /
xEdit documented "rotate around X first, then Y, then Z" convention):

```
Bethesda CW (Z-up):  R = Rz_cw(rz) · Ry_cw(ry) · Rx_cw(rx)
Standard CCW:        R = Rz_std(-rz) · Ry_std(-ry) · Rx_std(-rx)
```

Each CW rotation by `t` equals a glam CCW rotation by `-t`. The Z-up→Y-up
conjugation `C: (x,y,z)_zup → (x,z,-y)_yup` then maps each axis rotation:

```
C · Rx(-rx) · C^T = Rx(-rx)     X axis is unchanged
C · Ry(-ry) · C^T = Rz(ry)      Y maps to -Z, double negate
C · Rz(-rz) · C^T = Ry(-rz)     Z maps to Y
```

Composing under the axis swap (composition order preserved) gives the
shipping formula in
[`crates/core/src/math/coord.rs`](../../crates/core/src/math/coord.rs):

```rust
pub fn euler_zup_to_quat_yup(rx: f32, ry: f32, rz: f32) -> Quat {
    Quat::from_rotation_y(-rz) * Quat::from_rotation_z(ry) * Quat::from_rotation_x(-rx)
}
```

**Canonical reference**: OpenMW's static-REFR placement in
`apps/openmw/mwrender/objectpaging.cpp:853-855` (the shared ESM3+ESM4 path
covering Oblivion / FO3 / FNV / Skyrim REFRs):
`Quat(rot.z,(0,0,-1)) * Quat(rot.y,(0,-1,0)) * Quat(rot.x,(-1,0,0))`. Each
negated axis encodes the CW-positive convention; the Z-Y-X order matches
Bethesda CK / xEdit.

> **History (2026-05-26).** The pre-`#386aabb4` ship used the *XYZ-product*
> variant (`Rx · Ry · Rz` in Z-up). It was empirically picked from a
> single-cell sign-off on `GSDocMitchellHouse` (2026-05-07), whose REFRs are
> dominated by Z-only rotations — and XYZ-product / ZYX-product produce
> **identical** results when only `rz` is non-zero. The two diverge for
> multi-axis REFRs (slope-tilted exterior walls, sloped architecture),
> producing displaced or 90°-rotated walls. The ZYX-product / OpenMW formula
> fixes that, and a pair of multi-axis regression pins in
> `crates/core/src/math/coord.rs` (`euler_multi_axis_matches_openmw_objectpaging`,
> `euler_zyx_order_pinned_by_rx_then_rz`) lock it in.

#### `--rotation-mode` diagnostic switch

REFR placement does **not** call the canonical helper directly; it goes
through a runtime A/B dispatcher in
[`byroredux/src/cell_loader/euler.rs`](../../byroredux/src/cell_loader/euler.rs)
(`euler_zup_to_quat_yup_refr`) so an operator can re-triage candidate
conventions without rewiring the engine. `--rotation-mode N` (default `1`,
clamped to `0..=3`, wired in `byroredux/src/main.rs`):

| Mode | Convention | Z-up product | Status |
|------|------------|--------------|--------|
| 0 | CW  | `Rx · Ry · Rz` | pre-2026-05-26 ship; kept for A/B only |
| **1** | **CW**  | **`Rz · Ry · Rx`** | **current ship — matches OpenMW** |
| 2 | CCW | `Rz · Ry · Rx` | diagnostic |
| 3 | CCW | `Rx · Ry · Rz` | diagnostic |

Non-REFR callers (XCLL directional lighting in `scene.rs`, `#380`) call the
canonical `euler_zup_to_quat_yup` directly, bypassing the dispatcher, so the
core helper remains the single source of truth.

## Transform Pipeline

### 1. NIF Import (`crates/nif/src/import/`)

The NIF scene graph is walked recursively in
[`crates/nif/src/import/walk/mod.rs`](../../crates/nif/src/import/walk/mod.rs).
At each node, transforms are composed in **Z-up space** by
`compose_transforms` in
[`crates/nif/src/import/transform.rs`](../../crates/nif/src/import/transform.rs)
using Gamebryo's matrix math:

```
world_translation = parent.rot * (parent.scale * child.trans) + parent.trans
world_rotation    = parent.rot * child.rot
world_scale       = parent.scale * child.scale
```

Rotation matrices are **sanitized once at parse time** (not per-composition)
in [`crates/nif/src/rotation.rs`](../../crates/nif/src/rotation.rs)
(`sanitize_rotation`, `#277`). For degenerate parent rotation matrices
(det far from 1.0, e.g. BSFadeNode roots with garbage data) SVD repair runs:
- **Max singular value < 0.01**: matrix is garbage, fall back to identity.
- **Max singular value ≥ 0.01**: extract nearest valid rotation via `U · Vt`
  (sign-correct the third column if `det < 0`).

Because the matrix is already a valid rotation by the time the import walk
runs, `compose_transforms` assumes valid input and does no per-node checks.

The composed world transform and local vertex data are converted to Y-up
independently at the import boundary:
- Vertex positions/normals → `zup_point_to_yup` → `(x, z, -y)`.
- Translation → `zup_point_to_yup` → `(x, z, -y)`.
- Rotation matrix → `zup_matrix_to_yup_quat` → `C · M · C^T` then a unit
  quaternion via the Shepperd method (fast path, det ∈ ~[0.93, 1.07]) or a
  nalgebra SVD repair (degenerate fallback, ~1% of matrices), always
  normalised (`#333`).

### 2. Cell Loading (`byroredux/src/cell_loader/`)

`cell_loader.rs` was split into the
[`byroredux/src/cell_loader/`](../../byroredux/src/cell_loader/) directory;
REFR placement now lives in `references.rs` / `refr.rs`, the spawn
composition in `spawn.rs`, and the Euler dispatcher in `euler.rs`.

Each REFR (placed reference) has a Z-up position and Euler rotation. These
are converted to Y-up (`byroredux/src/cell_loader/references.rs`):

```rust
let outer_pos = Vec3::new(position[0], position[2], -position[1]);
let outer_rot = euler_zup_to_quat_yup_refr(rotation[0], rotation[1], rotation[2]);
let outer_scale = placed_ref.scale;
```

The per-REFR transform is then composed with the NIF-internal Y-up
transforms in `spawn.rs`:

```rust
let final_pos   = ref_rot * (ref_scale * nif_pos) + ref_pos;
let final_rot   = ref_rot * nif_quat;
let final_scale = ref_scale * mesh.scale;
```

The same `outer_rot * (outer_scale * local_pos) + outer_pos` composition is
reused when SCOL / PKIN composite records fan out child placements
(`refr.rs`), so nested static collections compose against the parent REFR
transform with one consistent policy.

### 3. Rendering (`crates/renderer/`)

The ECS `Transform` component stores Y-up translation, rotation (quat), and
uniform scale. Its model matrix is built in
[`crates/core/src/ecs/components/transform.rs`](../../crates/core/src/ecs/components/transform.rs)
(`Transform::to_matrix`):

```rust
Mat4::from_scale_rotation_translation(Vec3::splat(self.scale), self.rotation, self.translation)
```

The geometry vertex shader
([`crates/renderer/shaders/triangle.vert`](../../crates/renderer/shaders/triangle.vert))
selects a rigid `inst.model` matrix (or a blended bone palette for skinned
verts) and applies it before the view-projection:

```glsl
vec4 worldPos = xform * vec4(inPosition, 1.0);
vec4 currClip = viewProj * worldPos;   // gl_Position is jittered from this for TAA
```

### 4. Projection

The camera builds its projection in
[`crates/core/src/ecs/components/camera.rs`](../../crates/core/src/ecs/components/camera.rs)
(`Camera::projection_matrix`), using `Mat4::perspective_rh` (Z in [0, 1],
Vulkan convention) with a Y-flip for Vulkan's inverted Y axis:

```rust
let mut proj = Mat4::perspective_rh(self.fov_y, self.aspect, self.near, self.far);
proj.col_mut(1).y *= -1.0;
```

The Y-flip reverses apparent triangle winding in clip space: CW triangles
from NIF data appear CCW after projection, matching the pipeline's
`front_face: COUNTER_CLOCKWISE` setting.

## Exterior cell grid

Exterior worldspace placement uses
[`crates/core/src/math/coord.rs`](../../crates/core/src/math/coord.rs):

- `EXTERIOR_CELL_UNITS = 4096.0` — one Bethesda exterior cell spans 4096
  world units on each side (a 32 × 33-vertex landscape grid at 128-unit
  spacing). This is spec-defined for every Gamebryo / Creation Engine title
  shipped to date (Oblivion → Starfield), so it is hard-coded rather than
  per-game. As of `#1112` / TD3-202 it is the sole source of truth; pre-fix
  the literal `4096.0` appeared in six places across `cell_loader/`,
  `streaming.rs`, and `crates/plugin/src/esm/cell/mod.rs` with at least one
  divergent Z-flip sign-bug history (TD3-110).
- `cell_grid_to_world_yup(gx, gy)` — composes the cell-size scale with the
  Z-up→Y-up flip in one step: `world = (gx·UNITS, 0, -(gy·UNITS))`. Bethesda
  `+Y` (north) maps to renderer `-Z`.

## Triangle Winding

NIF triangle data uses **CW front face** (D3D convention). The full
winding chain:

1. NIF stores CW triangles.
2. Z-up → Y-up vertex conversion: det(C) = +1, winding preserved (still CW).
3. Model/view transforms: right-handed, det = +1, winding preserved (still CW).
4. Projection Y-flip: reverses apparent winding (CW appears CCW).
5. Pipeline front face: CCW = front → NIF faces are front-facing.

Backface culling is enabled (`CullModeFlags::BACK` with
`FrontFace::COUNTER_CLOCKWISE`) in the opaque/blend pipelines
([`crates/renderer/src/vulkan/pipeline.rs`](../../crates/renderer/src/vulkan/pipeline.rs)).

Cull mode is **dynamic state** (`vk::DynamicState::CULL_MODE`), not a baked
pipeline variant (`#930`). `draw.rs` issues `cmd_set_cull_mode(NONE)` for
two-sided batches and `cmd_set_cull_mode(BACK)` otherwise, splitting the
two-sided alpha-blend case into separate draws so transparency sorts
correctly. Two-sidedness is sourced from `NiStencilProperty` on FO3/FNV or
`SF_DOUBLE_SIDED` on Skyrim+/FO4 and carried on the render batch
(`batch.two_sided`).

## NiTriStrips Winding

Triangle strips alternate winding per-triangle to maintain a consistent
front face direction. The `to_triangles()` conversion lives on the strip
data in
[`crates/nif/src/blocks/tri_shape/ni_tri_shape.rs`](../../crates/nif/src/blocks/tri_shape/ni_tri_shape.rs)
and uses the **OpenGL/Vulkan** (CCW front) convention — swap the **last two**
vertices on odd triangles (D3D would swap the first two and yield CW):

```rust
for i in 2..strip.len() {
    let (a, b, c) = if i % 2 == 0 {
        (strip[i-2], strip[i-1], strip[i])   // even: standard order
    } else {
        (strip[i-2], strip[i], strip[i-1])   // odd: swap last two (CCW)
    };
    // degenerate (stitch) triangles where two indices match are skipped
}
```

Each strip resets the alternation independently (`i` starts at 2 per strip),
and degenerate stitch triangles (`a == b`, `b == c`, or `a == c`) are dropped.

## UV Coordinates

NIF UV coordinates use a top-left origin (U right, V down), matching
Vulkan's texture coordinate convention. No conversion needed.

## Summary Table

| Data                | Source Space      | Conversion           | Result Space    |
|---------------------|-------------------|----------------------|-----------------|
| Vertex position     | NIF local, Z-up   | `(x, z, -y)`         | Y-up local      |
| Vertex normal       | NIF local, Z-up   | `(x, z, -y)`         | Y-up local      |
| NIF translation     | NIF world, Z-up   | `(x, z, -y)`         | Y-up world      |
| NIF rotation matrix | NIF world, Z-up   | `C·M·C^T` → quat     | Y-up world      |
| NIF quaternion key  | NIF, Z-up (w,x,y,z)| `(x, z, -y, w)` + normalise | Y-up (x,y,z,w) |
| REFR position       | ESM world, Z-up   | `(x, z, -y)`         | Y-up world      |
| REFR Euler angles   | ESM world, Z-up CW| Negate + conjugate (ZYX) | Y-up quat (CCW) |
| Cell grid (gx, gy)  | ESM grid          | `(gx·4096, 0, -gy·4096)` | Y-up world  |
| UV coordinates      | NIF, top-left     | None                 | Vulkan, top-left|
| Triangle indices    | NIF, CW front     | None                 | CW → CCW via Y-flip |
