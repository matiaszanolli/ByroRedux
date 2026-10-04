# #5235: RT-2026-10-03-01: fo4 InstituteBioScience batches/gpu_calls 723→700 / 110→108 and skyrim_se gpu_calls 35→34 improved at `3c197ed8c` (#5057) without a TSV refresh

**Labels**: bug, low, performance, tech-debt, game:fo4, game:skyrim · **URL**: https://github.com/matiaszanolli/ByroRedux/issues/5235

**Source**: `docs/audits/AUDIT_RUNTIME_2026-10-03.md` RT-1 · **Severity**: LOW · **Dimension**: Baseline integrity · **Game / Cell**: fo4 / InstituteBioScience; skyrim_se / WhiterunDragonsreach

**Location**: `.claude/audit-baselines/runtime/fo4-InstituteBioScience.tsv` rows `bench_draws_batches`, `bench_draws_gpu_calls`; `.claude/audit-baselines/runtime/skyrim_se-WhiterunDragonsreach.tsv` row `bench_draws_gpu_calls`

| | Baseline | Current (HEAD `2c36c29d8`) |
|---|---|---|
| fo4 batches / gpu_calls | 723 / 110 | **700 / 108** |
| skyrim_se gpu_calls | 35 | **34** |

## Description
`3c197ed8c` (Fix #5057) admits lighting-shader material kinds 1–16 to early fragment tests. That changes which pipeline a draw binds, so adjacent draws merge into fewer batches and indirect calls. The rows pass the `≤ baseline ×1.1` gate, but the loose gate now has headroom: FO4 could slide back to 723b/110c, or anywhere up to 795b/121c, and still read green. The baselines README rule is to commit the TSV diff in the same commit as the engine change. `3c197ed8c` touches no TSV.

## Evidence
Attribution probes (renderer-static, 240 frames, `capture.sh`, same camera pose on both sides):

| Commit | fo4 InstituteBioScience | skyrim_se WhiterunDragonsreach |
|---|---|---|
| `8dbe179ad` (= `3c197ed8c^`) | 16885 · 3971/**723b/110c** r3618 | 9499 · 2494/643b/**35c** r1786 |
| `3c197ed8c` | 16885 · 3971/**700b/108c** r3618 | 9499 · 2494/643b/**34c** r1786 |

`8dbe179ad` reproduces the committed baselines bit-for-bit; entities, `cmds` and raster prefix are unchanged; only batch/call merging moved.

## Impact
None on rendering. The cost is that the gate cannot see a future batching regression of up to ~3 %, or the return of #5057's pre-fix pipeline split.

## Related
#5057 (closed by `3c197ed8c`); #4420 (precedent for tightening improved rows)

## Suggested Fix
`/audit-runtime --game fo4 --regen` and `--game skyrim_se --regen`. Write one header block per TSV naming `3c197ed8c`, and keep all four draw rows from the same capture.

## Completeness Checks
- [ ] **SIBLING**: The other baseline TSVs (and their newest `# regenerated:` block) checked for the same pattern
- [ ] **TESTS**: `cargo test -p byroredux --bin byroredux bench::` (runtime baseline schema tests) stays green after the edit
