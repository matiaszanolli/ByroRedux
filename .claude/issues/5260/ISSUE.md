# #5260: NIF-D3-2026-10-05-02: `block_coverage_baselines` records stale `total_blocks` on three titles, with no corpus-drift check; `parse_real_nifs` still carries the pre-#3461 "do not regenerate" NOTE

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/5260
- **Labels**: low,nif-parser,nif,documentation,doc-rot,test-gap,game:fo4,game:fo76,game:skyrim
- **Source**: `docs/audits/AUDIT_NIF_2026-10-05.md` (NIF-D3-2026-10-05-02)

_From `docs/audits/AUDIT_NIF_2026-10-05.md` (2026-10-05 comprehensive audit suite, HEAD a2c24b16e)._

- **Severity**: LOW (doc/test hygiene)
- **Dimension**: Block Dispatch Coverage (baseline harness)
- **Game Affected**: Fallout 4, Fallout 76, Skyrim SE
- **Location**: `crates/nif/tests/data/block_coverage_baselines/{fallout_4,fallout_76,skyrim_se}.tsv`; `crates/nif/tests/block_coverage_baselines.rs:322-327`; `crates/nif/tests/parse_real_nifs.rs:558-563`
- **Status**: NEW. #4628 (closed) added drift detection to `per_block_baselines` only.
- **Description**:
  - **Stale totals**: the coverage TSVs record `total_blocks` of 740,562 (FO4), 1,548,202 (FO76) and 758,733 (SSE). The regenerated per-block sums are 805,148, 1,770,751 and 856,103. FO3, FNV and Starfield match exactly. `run_unknown_ceiling` reads only `unknown_blocks`, so the stale totals never warn or fail. #4628's `baseline_corpus_total` drift check was not mirrored here.
  - **Stale NOTE**: separately, the NOTE (#3466) says the baselines must not be regenerated "before #3461 lands", because doing so "would bake FO76's 112,716 `NiUnknown` blocks into the accepted ceiling". #3461 landed on 2026-09-02, and #4628 regenerated `fallout_76.tsv` with 0 unknown blocks.
- **Impact**: none on gating, since the ceilings are 0 and correct. Readers are misled about corpus size, and about whether the FO76 baselines may be regenerated.
- **Related**: #4628, #3461, #3466, #5078.
- **Suggested Fix**: regenerate the three coverage TSVs, or drop the unread `total_blocks` line. Mirror the `total=` drift check, and delete or rewrite the #3466 NOTE.

**Publish note**: the report's baseline section verifies that #4628 (8c16fec42) also fixed open #5078. Close #5078 alongside this one.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
