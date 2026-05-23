# #1232 — REN-D16-NEW-02: BSGeometry no-tangent fallback returns Vec::new() instead of synthesize_tangents_yup

**Source**: `docs/audits/AUDIT_RENDERER_2026-05-23_DIM16.md`
**Severity**: MEDIUM
**Dimension**: Tangent-Space & Normal Maps

## Symptom

Starfield meshes (BSGeometry) lacking authored UDEC3 tangents fall through to the screen-space derivative TBN fallback (`perturbNormal` Path 2) and inherit the open #1104 UV-mirror handedness bug. The `synthesize_tangents_yup` helper (added under the #1204 BSTriShape follow-up) is available but the BSGeometry slot doesn't route through it.

## Cause

[crates/nif/src/import/mesh/bs_geometry.rs:113-120](https://github.com/matiaszanolli/ByroRedux/blob/main/crates/nif/src/import/mesh/bs_geometry.rs#L113-L120):

```rust
let tangents: Vec<[f32; 4]> = if !mesh_data.tangents_raw.is_empty() {
    mesh_data.tangents_raw.iter().map(|&raw| { … }).collect()
} else {
    // No authored tangents — the renderer falls back to screen-space
    // derivative TBN (Path 2). A future improvement could call
    // synthesize_tangents here, but it requires a Y-up variant since
    // BSGeometry data is already in engine space (unlike the Z-up
    // input that NiTriShape / BSTriShape synthesis expects).
    Vec::new()
};
```

The Y-up variant the comment is waiting on shipped as `synthesize_tangents_yup` at [tangent.rs:376-516](https://github.com/matiaszanolli/ByroRedux/blob/main/crates/nif/src/import/mesh/tangent.rs#L376-L516). The BSTriShape SSE-reconstructed branch at [bs_tri_shape.rs:178-189](https://github.com/matiaszanolli/ByroRedux/blob/main/crates/nif/src/import/mesh/bs_tri_shape.rs#L178-L189) routes through it (#1204); the parallel BSGeometry slot was missed in that follow-up.

## Fix

Mirror the BSTriShape Path-D branch:

```rust
} else if !normals.is_empty() && !uvs.is_empty() && !positions.is_empty() {
    let triangles_u16: Vec<[u16; 3]> = indices
        .chunks_exact(3)
        .filter_map(|c| {
            if c[0] <= u16::MAX as u32 && c[1] <= u16::MAX as u32 && c[2] <= u16::MAX as u32 {
                Some([c[0] as u16, c[1] as u16, c[2] as u16])
            } else { None }
        })
        .collect();
    synthesize_tangents_yup(&positions, &normals, &uvs, &triangles_u16)
} else {
    Vec::new()
};
```

Strip the stale "future improvement" comment in the same hunk.

## Estimated Impact

Direct quality win on Starfield mesh content that omits UDEC3 tangents (vanilla `Saturn.nif` ships authored, but mod content + some LOD chains do not). Closing this turns those meshes from Path-2 fallback (which carries #1104's UV-mirror sign bug) into Path-1 — eliminates the audit-chain dependency.

## Regression Risk: LOW

`synthesize_tangents_yup` is already in production use via BSTriShape (#1204); the math is unit-tested by `synthesize_tangents_yup_stores_dpdu_not_dpdv` + `synthesize_tangents_yup_flips_bitangent_sign_on_mirrored_uvs` in `tangent_convention_tests.rs`. The change is a wiring patch around the existing function.

## Related

- #1086 (CLOSED) — original BSGeometry UDEC3 decode for populated-tangents case
- #1204 (CLOSED) — sibling BSTriShape SSE-recon fix that this finding mirrors
- #1104 (OPEN) — Path-2 UV-mirror handedness bug that BSGeometry currently inherits

## Completeness Checks

- [ ] **UNSAFE**: N/A — pure-Rust arithmetic in `synthesize_tangents_yup`
- [ ] **SIBLING**: verify no other BS* path (BSEffectShader meshes? Future SF blocks?) has the same `Vec::new()` no-tangent fallback that should route through the Y-up sibling
- [ ] **DROP**: N/A — no resource-lifecycle change
- [ ] **TESTS**: `bs_geometry_tangent_tests.rs` regression test asserting `synthesize_tangents_yup` fires when `tangents_raw` is empty + `normals` / `uvs` / `positions` are populated
