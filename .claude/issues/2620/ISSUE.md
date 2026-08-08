# SF2D2-D2-02: weights_per_vert==0 with nonzero n_total_weights reads zero bytes, drifts rest of .mesh parse

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2620
**Finding ID**: SF2D2-D2-02

**Severity**: MEDIUM
**Dimension**: 2 (BSGeometry Mesh Extraction)
**Location**: `crates/nif/src/blocks/bs_geometry.rs:479-495`
**Status**: NEW

## Description
`n_total_weights.checked_div(weights_per_vert)` returns `None` only for
`weights_per_vert == 0`, and that arm reads **zero bytes** regardless of
`n_total_weights`. If a `.mesh` body ever ships `weights_per_vert == 0` with
`n_total_weights > 0`, the undrained `BoneWeight` payload shifts every
subsequent field (`n_lods`/`n_meshlets`/`n_cull_data`) into garbage, driving
`read_u16_triple_array` off a corruption-controlled count.

## Evidence
`crates/nif/src/blocks/bs_geometry.rs:479-495` — the `weights_per_vert == 0`
arm reads nothing instead of skipping the payload while advancing the
cursor.

## Impact
Parse-position drift on malformed/atypical `.mesh` bodies (per the severity
table, MEDIUM for "stream position off"). Bounded by `check_alloc` (no
OOM/UB), but the mesh silently loses its LOD/meshlet/cull tables or fails,
surfacing only as "REFR spawned with zero meshes" with no diagnostic —
Stage B's error arm is `log::debug!`-only.

## Suggested Fix
Treat `weights_per_vert == 0` as "skip the payload, still advance the
cursor" (`stream.skip(n_total_weights * 4)`), not "read nothing." Add a unit
test with `weights_per_vert = 0`, `n_total_weights = 2`, a non-zero `n_lods`
following.

## Related
The deliberate remainder case (`n_total_weights % weights_per_vert != 0`) is
correctly pinned by `skin_weights_bulk_read_matches_per_element_semantics`;
that test does not cover the `== 0` arm.

## Completeness Checks
- [ ] **TESTS**: A `weights_per_vert=0, n_total_weights=2` fixture with a non-zero trailing `n_lods` pins the cursor-skip fix
