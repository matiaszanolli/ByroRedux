# REN-D23-02: the FSR bench harness changed its measurement conditions *and* its TSV schema in `f19f7f15` without a re-bench, and `fsr_bench_report.py` now crashes on its own committed archive

- **Severity**: MEDIUM
- **Dimension**: 23 — FSR Upscaler (bench-harness stability)
- **Location**: `scripts/fsr-bench-matrix.sh` (the `--bench-mode renderer-stepped --bench-camera "$CAMERA_PATH"` addition and the widened `printf` header) and `scripts/fsr_bench_report.py` (`main`), against `docs/audits/BENCH_R6a-stale-17_head_3a02b02d.tsv` and `docs/audits/BENCH_R6a-stale-17_control_e153b50c.tsv`
- **Status**: NEW
- **Description**: `git log --oneline -- scripts/fsr-bench-matrix.sh scripts/fsr_bench_report.py` returns exactly two commits: `e153b50c` (2026-07-24) and `f19f7f15` (2026-08-11). That second commit changed three things at once: (a) every run now executes `--bench-mode renderer-stepped --bench-camera "$CAMERA_PATH"` instead of the previous parked-camera capture, so the workload being measured is different; (b) the TSV gained six columns (`mode`, `camera`, `sim_time_s`, `lights`, `tlas`, `state_hash`); (c) the report script gained a scene-state fingerprint gate that indexes those columns unconditionally. No bench table was refreshed in or after `f19f7f15` — `git log f19f7f15..HEAD -- ROADMAP.md docs/engine/fsr3-upscaler-integration-plan.md docs/audits/BENCH_*.tsv` returns only the session-65 closeout, which touched neither. Every published FSR number therefore describes the pre-`f19f7f15` harness and cannot be compared against any run of the current one.
- **Evidence**: Reproduced against current `main` (`e4ab12e8`): `python3 scripts/fsr_bench_report.py docs/audits/BENCH_R6a-stale-17_head_3a02b02d.tsv` raises `KeyError: 'mode'` at line 102. Both committed TSVs still carry the 17-column pre-`f19f7f15` header while `fsr-bench-matrix.sh` now emits 23 columns.
- **Impact**: The two artefacts the repo keeps specifically so cross-commit FSR comparisons stay checkable are unreadable by the tool that produced them, and the phase-7 net-frame-recovery table — the stated justification for FSR Quality being the engine default — has no reproducible path. The methodology change is itself defensible (`docs/engine/fsr3-troubleshooting.md` argues a parked camera hides disocclusion failures, and `f19f7f15` did update that doc), but it landed without re-taking the baseline it invalidates. ROADMAP.md independently flags the bench-of-record as 116 commits stale and "unreliable", so there is currently no live FSR bench of any kind — which blocks measuring REN-D23-01's fix.
- **Related**: #2560, #2084, #2279 (all closed, same bench-staleness class); ROADMAP.md R6a-stale-19; REN-D23-01 (#2834, blocked on this finding's fix).
- **Suggested Fix**: Re-run `scripts/fsr-bench-matrix.sh` on a current HEAD and replace the phase-7 table with the stepped-camera figures, labelling the old table with the harness commit it was taken on. Make `fsr_bench_report.py` tolerate a missing column (`row.get(key, "-")`) so the committed historical TSVs stay readable, or archive them with an explicit harness-commit header line. No FPS or ms figure is asserted anywhere in this report; that is the point of the finding.

## Completeness Checks
- [ ] **SIBLING**: Check `fsr_bench_report.py` for any other column accessed by bare `row[...]` indexing that would break the same way against an older-schema TSV
- [ ] **TESTS**: A regression feeds `fsr_bench_report.py` a pre-`f19f7f15` 17-column TSV and asserts graceful handling (no `KeyError`) instead of a crash

---
**Source**: `docs/audits/AUDIT_RENDERER_2026-08-12b.md` (finding `REN-D23-02`)
**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2835
