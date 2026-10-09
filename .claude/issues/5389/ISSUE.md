# #5389: FO4-2026-10-08-D3-01: BSTriShape skin producer (FO4 + Skyrim SE) passes packed `u8` bone indices through unbounded; #4268 fixed only the Starfield sibling, on a false "siblings already bound" premise

**Labels**: medium,nif-parser,nif,import-pipeline,safety,bug,game:fo4,game:skyrim,legacy-compat
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5389

**Source**: `docs/audits/AUDIT_FO4_2026-10-08.md` — `FO4-2026-10-08-D3-01` (HEAD `00f580e09`)

- **Severity**: MEDIUM. This is a defense-in-depth gap of the same class #4268 was filed and fixed at. It is not
  reachable on vanilla FO4 (measured below).
- **Dimension**: NIF BSVER 130 geometry, skinned indices/weights. The mechanism owner is `/audit-nif`; FO4 and Skyrim SE
  are the reach.
- **Location**:
  - `crates/nif/src/import/mesh/skin.rs:201-210`: `extract_skin_bs_tri_shape`. The inline and SSE global-buffer arms
    both widen raw indices.
  - `crates/nif/src/import/mesh/skin.rs:576-581`: `widen_packed_bone_indices` is a pure `u8 → u16` widen with no bound.
  - The returned `ImportedSkin`s at `skin.rs:244-252` (NiSkinInstance arm) and `:276-297` (BSSkin::Instance arm, the FO4
    path) never compare an index against `bone_refs.len()`.
- **Status**: NEW. #4268 (closed, 0abc86c8b) covers only `convert_bs_geometry_skin_weights`. The 2026-10-08 NIF, NIFAL,
  SAFETY and SKYRIM reports each verify #4268 and say it touches only the Starfield producer. None of them checks the
  BSTriShape sibling.
- **Description**:
  - `render/skinned.rs:62-66` states a structural invariant: "a vertex's bone-weight index is bounded by its own mesh's
    bone count at import time, so it structurally cannot reach a reused or unallocated slot's stale content". That
    invariant is why the palette tail is no longer re-identity-filled each frame (#1794).
  - #4268's issue body says the BSGeometry producer is the one that is unbounded, "unlike its sibling
    BsTriShape/NiTriShape decoders". Its fix commit says "the one per-vertex bone-index producer that passed … through
    unbounded".
  - Both statements are false for BSTriShape. Its packed `bone_indices: Vec<[u8; 4]>` (`bs_tri_shape.rs:116`), and the
    SSE `SseSkinGlobalBuffer` payload, reach `ImportedSkin.vertex_bone_indices` unchecked. That covers every FO4 skinned
    actor, creature and power-armor mesh, and every Skyrim SE body.
  - `nif_loader.rs:1084-1096` copies the indices straight into `Vertex::new_skinned_rgba`.
- **Evidence**:
  - The code path:
    ```rust
    // skin.rs:201-210 — no bound against bone_refs / bone_data.bones
    (shape.bone_weights.clone(), widen_packed_bone_indices(&shape.bone_indices))
    // skin.rs:576-581
    pub fn widen_packed_bone_indices(bone_indices: &[[u8; 4]]) -> Vec<[u16; 4]> {
        bone_indices.iter().map(|indices| indices.map(u16::from)).collect()
    }
    ```
    Compare `skin.rs:337-346`, where the #4268 producer declines on `bw.bone_index as usize >= bone_count`.
  - The GPU side clamps only to `MAX_BONES_PER_MESH - 1` (`skin_vertices.comp:207`). An index in
    `[bone_count, 144)` therefore reads the occupant's own palette tail, which `skinned.rs:211-217` leaves stale on
    purpose. Indices of 144 to 255 clamp to slot 143.
  - Vanilla census, using the scratch probe: `import_nif_scene` was run over all 8 FO4 mesh BA2s, 235,082 NIFs.
    - 23,099 skinned meshes and 18,630,555 skinned vertices.
    - **0 out-of-range influences**, whether weighted or zero-weight.
    - The largest bone list is 141, against `MAX_BONES_PER_MESH = 144`.
  - Skyrim SE reach was not measured.
- **Impact**:
  - A mod or malformed FO4/SSE NIF with an index at or above its bone count deforms through a stale matrix from a prior
    slot occupant, or through identity on first growth. The bug is visual only.
  - There is no out-of-bounds GPU read and no UB, because the shader clamp keeps the read inside the slot.
  - A zero-weight out-of-range index still multiplies that stale matrix by 0. If the stale matrix holds non-finite
    values, the result is NaN.
  - The same documented invariant #4268 restored for Starfield stays broken for FO4 and SSE.
- **Related**: #4268 (closed; its sibling premise is false), #1794 (the stale-tail rationale), #2467
  (`bind_unweighted_to_bone_zero`, which runs on this same path).
- **Suggested Fix**: Thread the resolved bone count (`bone_refs_slice.len()`, or `inst.bone_refs.len()`) into the
  BSTriShape arm and apply #4268's decline-whole-weight-set rule: any index at or above the count empties both arrays,
  and the mesh falls back to bind pose. Add the out-of-range and `bone_count - 1` boundary tests from
  `bs_geometry_skin_tests.rs`. Also correct the `render/skinned.rs:62-66` wording if any producer stays unbounded.

## Completeness Checks
- [ ] **SIBLING**: Both BSTriShape arms (inline arrays and SSE `SseSkinGlobalBuffer` payload) bounded; the classic NiTriShape/NiSkinData producer re-checked against the same `bone_count` rule; `render/skinned.rs` invariant wording matches
- [ ] **TESTS**: A regression test pins this specific fix (out-of-range index and `bone_count - 1` boundary, mirroring `bs_geometry_skin_tests.rs`)
