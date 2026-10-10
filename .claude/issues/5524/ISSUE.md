# #5524: NIF-D4-2026-10-09-02: #5389 follow-through has three gaps: its doc insertion orphaned `widen_packed_bone_indices`' doc comment, its warning mislabels the classic arm, and the FO4 `BsSkinInstance` arm it targets has no test

**Labels**: bug, low, nif, nif-parser, test-gap

**Source**: `docs/audits/AUDIT_NIF_2026-10-09.md` — finding `NIF-D4-2026-10-09-02` · suite `/audit-suite --preset streaming-deep` (2026-10-09) · HEAD `3bcf6c8e8`

- **Severity**: LOW (doc rot plus a test gap. The code is correct, and the census confirms it.)
- **Dimension**: Geometry Extraction & Import Handoff
- **Game Affected**: FO4, Skyrim SE (the `BsSkinInstance` arm untested). Doc only for the rest.
- **Location**:
  - `crates/nif/src/import/mesh/skin.rs:595-611`: the original `widen_packed_bone_indices` doc, lines `:595-605` (nifly `OptimizeFor` / Draugr evidence), is now directly followed by the #5389 lines `:606-611` and attaches to the private `decline_unbounded_packed_indices` (`:612`). The public `widen_packed_bone_indices` (`:627`) is left with no doc.
  - `:618-621`: the warning says "BSTriShape skin: …" but is also emitted from `extract_skin_ni_tri_shape` (`:156`). NIFAL's 2026-10-09 report noted this as cosmetic.
  - `crates/nif/src/import/mesh/bs_tri_shape_partition_remap_tests.rs:401-560`: both new tests build a `NiSkinInstance`. There is no test for the `BsSkinInstance` arm (`skin.rs:301`, every FO4 skinned actor, which the commit message names), and none for the SSE `SseSkinGlobalBuffer` route.
- **Status**: NEW (follow-through of #5389, which is closed).
- **Description**: the decline logic is right. It covers all four lanes, including zero-weight lanes, and never clamps. The classic arm is provably unreachable: `densify_sparse_weights` emits enumerate-positions in `data.bones`, and `skin.rs:135` already enforces `data.bones.len() == bone_refs.len()`. That makes it harmless defense. The census shows no vanilla mesh trips the decline: 0 declines over 72,538 skinned meshes across FNV, FO3, Oblivion, LE, SSE and FO4, and 0 out-of-range lanes, weighted or zero-weight, in FO4's 16,343 inline skinned `BSTriShape`s.

  The gaps are only in documentation and coverage:
  - the misplaced doc block;
  - the mislabelled log;
  - a decline that is untested on the arm it was written for. A future refactor of that arm could drop the call without any test failing.
- **Impact**:
  - Rustdoc and readers attribute the packed-index-space rationale (#613/#2577) to the wrong function.
  - A future refactor of that arm could silently drop the bound. The render-side palette invariant (`render/skinned.rs`) relies on it.
- **Related**: #5389, #4268, #613, #2577.
- **Suggested Fix**:
  - Move the #5389 doc lines and the function below `widen_packed_bone_indices`, restoring its doc.
  - Make the log prefix arm-neutral, for example "skin:".
  - Add a decline test and a `bone_count - 1` boundary test that build a `BsSkinInstance` + `BsSkinBoneData` shape. Optionally add an SSE global-buffer variant.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other walkers / record decoders / games / call sites)
- [ ] **TESTS**: A regression test pins this specific fix
