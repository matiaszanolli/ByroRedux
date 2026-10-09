# #5438: NIF-D3-2026-10-08-01: #5260's `total_blocks` pin measures parser output, not the install, so a truncation regression fails as "corpus drift — regenerate"

**Labels**: low,nif-parser,nif,bug,test-gap
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5438

**Source**: `docs/audits/AUDIT_NIF_2026-10-08.md` — `NIF-D3-2026-10-08-01` (HEAD `00f580e09`)

- **Severity**: LOW (test-gap: misattribution in a gate. The per-block gate still catches the real loss.)
- **Dimension**: Block Dispatch Coverage (baseline harness)
- **Game Affected**: all seven `run_unknown_ceiling` titles: FO3, FNV, SSE, FO4, FO76, Starfield. Oblivion uses its own parity test.
- **Location**:
  - `crates/nif/tests/block_coverage_baselines.rs:289-303` (`measure_coverage`; `total_blocks` is the sum of `parsed + unknown` from `scene.blocks`);
  - `:383-394` (the #5260 assert and its message).
- **Status**: NEW. #5260 (closed) introduced the check. It was meant to mirror #4628 but does not.
- **Description**: #4628's `baseline_corpus_total` check in `per_block_baselines` compares the TSV header's `total=`. That value is the **NIF file count** (`to_tsv(total_files)`), which the install alone determines. #5260's coverage-side check instead compares `total_blocks`, which is the count of blocks the parser **returned**. `record_scene_blocks` iterates `scene.blocks`, and a hard `parse_nif` error records nothing.
  - So a parser regression that truncates scenes or hard-fails files lowers `total_blocks`. The gate then fails *before* the ceiling comparison, with: "corpus drift … the game data moved under the baseline … regenerate with `BYROREDUX_REGEN_BASELINES=1`".
  - A truncation fix that recovers blocks also fails, with the same message.
- **Evidence**: `measure_coverage` → `parse_archive_with_histogram` → `hist.record_scene_blocks(header, &scene.blocks)`. Then `total_blocks = Σ(parsed + unknown)`, and `assert!(cov.total_blocks == baseline_total, "[…] corpus drift: …")`. The per-block side reads `if baseline_total != total_files` (`per_block_baselines.rs:194-195`).
- **Impact**: the gate tells the operator that a parser regression is install drift and prescribes a regenerate. That regenerate would bake the lower total into the baseline. The NiUnknown ceiling is unaffected, and `per_block_baselines` would still flag `PARSED shrank` for the same run, so this is diagnosis hygiene, not a hole in coverage.
- **Related**: #5260, #4628, #2334.
- **Suggested Fix**: pin the walked NIF-file count, which the install determines, as the corpus total. Report a `total_blocks` change separately as a parser-side delta. Alternatively, record the file count in the coverage TSV and compare that.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (`per_block_baselines` corpus-total check and any other baseline that keys on parser output)
- [ ] **TESTS**: A regression test pins this specific fix
