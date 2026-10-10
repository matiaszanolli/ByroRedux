# #5484: FO4-2026-10-09-D1-01: XCRI/XPRI semantics are inverted at the precombine de-dup gate, so every baked STAT/SCOL is drawn twice and the unbaked XPRI statics vanish

**Labels**: bug, esm-plugin, game:fo4, high, legacy-compat, renderer

**Source**: `docs/audits/AUDIT_FO4_2026-10-09.md` — finding `FO4-2026-10-09-D1-01` · suite `/audit-suite --preset streaming-deep` (2026-10-09) · HEAD `3bcf6c8e8`

- **Severity**: HIGH. This is rendering correctness and GPU memory on every precombined FO4 cell: duplicate coincident
  geometry in raster and in the BLAS/TLAS, plus deleted content.
- **Dimension**: 1. This is the REFR de-dup gate (#1188). The parse half is in `crates/plugin/src/esm/cell/helpers.rs`
  (owner `/audit-esm`), and FO4's data is the reach.
- **Location**:
  - `crates/plugin/src/esm/cell/helpers.rs:119-160`: the XCRI arm keeps only the mesh hashes. It explicitly refuses the
    reference tail as a "visibility group".
  - `crates/plugin/src/esm/cell/helpers.rs:162-180`: XPRI becomes `absorbed_refs`.
  - `byroredux/src/cell_loader/references/mod.rs:536-553`: only `absorbed_refs` STAT/SCOL are skipped.
  - `byroredux/src/cell_loader/references/mod.rs:163-202`: the `precombine_can_replace_record` doc.
  - `crates/plugin/src/esm/cell/mod.rs:312-335`: the `CellData` docs.
  - `byroredux/src/cell_loader/precombined.rs:106-129`: the `absorbed_refs_or_empty` gate, shared by `load.rs:617`,
    `load.rs:992` and `exterior.rs:2271`.
- **Status**: NEW.
  - #1188 (closed) introduced the XPRI-as-skip-list premise.
  - #2699 (closed, doc-only via 32c8e6e6c) recorded the contract as "unsettled".
  - The 2026-06-02 report's FO4-D9-DOC-03, which was never published, is subsumed. It flagged the `CellData` doc's XCRI
    wording.
  - No open issue covers this.
- **Description**:
  - xEdit's FO4 definition is the authority here (`wbDefinitionsFO4.pas:6087-6097`):
    - `XPRI` is `'PreVis Reference Index'`, a plain FormID array.
    - `XCRI` is `'Combined Reference Index'`: `Meshes Count`, `References Count`, `Meshes[u32]`, then
      `References[{Reference FormID, Combined Mesh u32}]`.
    - `References Count` counts both struct members, so it is 2 × entries (`wbCELLCombinedRefsCounter`, `:2429-2437`).
      That is why the parser's `8 + mc*4 + rc*4` size check passes.
  - The XCRI reference tail is therefore the list of references the CK *combined* into the `_oc.nif` bakes, each one
    paired with its combined-mesh hash. XPRI is the previs participant list.
  - The engine does the opposite:
    - It drops the XCRI pairs.
    - It treats XPRI as "absorbed". `references/mod.rs:276-281` even states the consequence of getting this set wrong:
      "Spawning here would produce double geometry + z-fighting on every wall / floor / ceiling".
- **Evidence**:
  - Census over `Fallout4.esm` (`/tmp/audit/fo4/py/xcri_xpri.py`):
    - 4,637 cells.
    - XCRI references: 959,238, made up of **STAT 924,448 and SCOL 34,789**. That is 100% the precombinable types.
    - XPRI: 40,750, made up of MSTT 14,381, STAT 9,175, CONT 8,718, FURN 6,394, ACTI 1,133, SCOL 711 and smaller
      groups.
    - **XCRI ∩ XPRI = 0.**
  - DLCs, XCRI references, all disjoint from XPRI:

    | DLC | XCRI references |
    |---|---|
    | DLCRobot | 65,761 |
    | DLCCoast | 211,198 |
    | DLCNukaWorld | 183,838 |
    | DLCworkshop03 | 24,881 |
    | DLCworkshop01 | 72 |

  - Geometry proof (probe `ocpos`): for each ref's `DATA` position, the nearest translation among the cell's decoded
    `BSPackedGeomDataCombined` instances. "Exact" means < 0.01 u.

    | Cell | Instances | XCRI exact | XPRI exact |
    |---|---|---|---|
    | Switchboard (`000b42e4`, 60 bakes) | 4,504 | STAT 2,277/2,278, SCOL 66/66 | STAT 0/33, CONT 0/41, FURN 0/27, ACTI 0/10, TERM 0/3, MSTT 10/60 |
    | InstituteBioScience (`00153797`) | 1,722 | STAT 493/493, SCOL 8/9 | 0/79 (STAT 0/2) |
    | Exterior `000a801e` | 541 | STAT 378/378, SCOL 2/2 | 0/42 (STAT 0/3) |

  - #2699's own 2026-09-08 measurement already pointed this way:
    - It found that the XPRI STAT meshes appear nowhere in the bake. It read that as "the control fails".
    - In fact that result was the evidence that XPRI refs are not baked.
- **Impact**: whenever `pc_spawned > 0` (every vanilla precombined cell, interior and exterior):
  1. **Every XCRI STAT/SCOL ref is spawned individually *and* drawn from the bake.**
     - This produces coincident duplicate triangles in raster, and duplicate BLAS/TLAS instances for RT. Coplanar twins
       are a self-intersection hazard for shadow and reflection rays.
     - It doubles the vertex/index uploads and the draw and BLAS counts.
     - It doubles the colliders: each REFR's own, plus the precombine's synthesized `ArchitectureTriMesh`.
     - Switchboard spawns 2,344 extra REFRs. InstituteBioScience, the FO4 runtime baseline cell, spawns 502.
     - On the streaming route, each FO4 exterior apply pays for its XCRI refs on top of the precombine: 380 on
       `000a801e`. That is a direct grid-cross readiness cost.
  2. **XPRI STAT/SCOL refs are suppressed, but they are not in the bake.** Those placements are deleted:
     - 9,886 in `Fallout4.esm`.
     - Switchboard 33, InstituteBioScience 2, `000a801e` 3.
  - Not verified, and not claimed as the cause: #5370 (Institute speckle) also reproduces in the data-free Cornell box
    (#5369), so duplicate geometry cannot be its sole mechanism. A before/after look at an FO4 interior once this is
    fixed would show whether it contributes.
- **Related**:
  - #1188 and #2699 (both closed).
  - #2698 (XPRI remap, closed; the remap stays correct for the new set).
  - #5309.
  - FO4-2026-10-09-D4-02: 278 of the 279 stale-MODL SCOL refs are XCRI-baked, so this fix removes most of that finding's
    reach.
  - `/audit-runtime`: the `fo4-InstituteBioScience.tsv` draw, entity and BLAS rows will move. Recapture them in the same
    commit.
- **Suggested Fix**:
  - Decode the XCRI `(reference, combined mesh)` pairs (remapped) into the skip set, and stop skipping XPRI. Keep the
    `pc_spawned == 0` fallback.
  - Keying each ref by its combined-mesh hash lets a single missing `_oc.nif` un-skip only its own refs.
  - Rewrite the three doc sites: `helpers.rs`, `cell/mod.rs` and `references/mod.rs:163-202`.
  - Pin the result with a real-data test: XCRI refs coincide with bake instances, XPRI refs do not.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other walkers / record decoders / games / call sites)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix
