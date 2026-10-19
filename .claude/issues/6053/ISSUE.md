# FNV-2026-10-18-04: `m-exteriors.sh fnv boundary` hard-fails the live LOD-coverage gate with 384 overlapping terrain-LOD pairs

- **Issue**: #6053 (https://github.com/matiaszanolli/ByroRedux/issues/6053)
- **Labels**: medium, terrain-exterior, bug, game:fnv, legacy-compat
- **Source**: docs/audits/AUDIT_FNV_2026-10-18.md (snapshot as filed; GitHub is authoritative for state)

Found by the FNV compatibility audit of 2026-10-18 (`docs/audits/AUDIT_FNV_2026-10-18.md`, HEAD `af5063597`).

- **Origin**: D1-03. **Owner**: `/audit-exterior`. **Not a regression**: identical numbers on the baseline build `5d31fd637`; onset unknown.
- **Location**: producer `byroredux/src/streaming_helpers.rs::update_lod_coverage` (`state.lod_blocks`); selection `byroredux/src/cell_loader/lod_bands.rs`; residency `terrain_lod*.rs`.
- **Status**: NEW. #2371 and #4504 / #4505 are the construction-proof tickets, not this live result.
- **Description**: Each of the 24 coarse terrain-LOD blocks (L16 / L32) stays resident alongside the ~16 finer L4 / L8 blocks inside its footprint (12 × 32 = 384).
- **Evidence**: `lod-coverage: sampled=1 settled=1 overlaps=384 full_detail_overlaps=0 vwd_full_model_overlaps=0 terrain_churn=0 object_churn=0 terrain_resident=460 object_resident=285 verdict=FAIL`, on HEAD and on `5d31fd637`. Instrumenting a baseline-tree copy printed key histogram `{object L4: 285, terrain L4: 238, L8: 196, L16: 12, L32: 12}`, terrain overlaps 384, object overlaps 0, e.g. `(8,-64,24) vs (32,-64,0)`. The earlier "FNV: 0 overlaps" census claims are about the authored object-LOD index, not this engine-generated terrain ring. `settled=1` and `terrain_churn=0`, so it is a stable state, not a transient.
- **Impact**: two terrain-LOD surfaces cover the same far-field ground after a boundary traversal. User-visible effect (z-fight or double draw) is unproven; the gate that guards it is red for the reference title.
- **Suggested Fix**: find why the ring keeps both a coarse block and the finer blocks it supersedes after the three crossings; run the same gate on Skyrim to see whether it is FNV-specific.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / the other FO3 / Oblivion / Skyrim paths that share this code
- [ ] **TESTS**: A regression test (or a repaired gate) pins this specific fix

