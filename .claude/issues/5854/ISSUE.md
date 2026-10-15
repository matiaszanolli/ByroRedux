# TD9-2026-10-15-02: The committed `combustion_lab_60f` golden last failed by a wide margin on this machine and was never regenerated; the golden CI job is manual-dispatch only

Labels: low,test-gap,renderer,tech-debt,bug

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD9-2026-10-15-02`).

- **Severity**: LOW (promote to MEDIUM if a run on the RT runner confirms red at HEAD)
- **Dimension**: 9 - Test Hygiene
- **Location**: `byroredux/tests/golden/combustion_lab_60f.png` (committed 2026-09-21, `7cdc0c7ff`), `byroredux/tests/golden/cube_demo_60f.png` (2026-09-15, `b23c92e2f`), `byroredux/tests/golden_frames.rs:102-107` (`PIXEL_TOLERANCE = 8`, `MAX_DIFF_PCT = 1.0`, `MAX_CHANNEL_DELTA = 32`), `.github/workflows/rt-correctness.yml:3-14,68-94`
- **Status**: NEW (#5625 closed by adding the `golden` job; #4227 closed by a regen)
- **Age**: baselines 2026-09-21 / 2026-09-15; 103 commits touched `crates/renderer/shaders` since 2026-09-21
- **Effort**: small
- **Description**: `byroredux/tests/golden/combustion_lab_60f.png.actual.png` (git-ignored, mtime 2026-10-09 17:12) is the artefact `save_actual_next_to_baseline` writes only when `compare_or_fail` rejects a frame. Compared to the committed baseline (both 1280x720): 98.9 % of pixels differ by more than 8/255 and the largest channel delta is 200/255, against thresholds of 1 % and 32. No regeneration followed (the PNG is unchanged since 09-21; 48 renderer commits landed after 10-09). Whether the failure was an intended visual change awaiting a regen, a different GPU, or a regression is not recoverable from the tree. The staleness gate `committed_baseline_matches_the_current_invocation` only checks that the `.capture` manifest names the current CLI flags, not that the pixels are current. The `golden` job exists (#5625) but `rt-correctness.yml` triggers on `workflow_dispatch` only and needs the self-hosted `byroredux-rt` runner, so nothing runs it on a push or a schedule.
- **Evidence**: `python3 -I` with PIL/numpy over the two PNGs: `pct>8: 98.89 max 200`. `ls -la byroredux/tests/golden`.
- **Impact**: the only pixel-level render regression guard is either red and ignored or green by luck; `audit-runtime` lists both goldens as a gate.
- **Related**: #5625, #4227, #3849, `audit-runtime` Dim (gate matrix).
- **Suggested Fix**: on the RT runner run `cargo test --release -p byroredux --test golden_frames -- --ignored`; regenerate with `BYROREDUX_REGEN_GOLDEN=1` after a human look at `.actual.png` if the change is intended, else bisect the 103 shader commits. Add a `schedule:` trigger to the `golden` job (nightly, like `real-data-gates.yml`).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
- [ ] **TESTS**: A regression test or guard pins this specific fix (or the existing guard is named)
