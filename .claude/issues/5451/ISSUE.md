# #5451: PERF-D8-2026-10-08-01: the bench harness pair was edited twice since the record, and the second edit changes a measured scene's inputs without a re-bench

**Labels**: low,performance,tech-debt,bug,game:fnv
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5451

**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-10-08.md` — `PERF-D8-2026-10-08-01` (HEAD `00f580e09`)

**Publish note**: Validated at HEAD: `scripts/check-bench-harness-provenance.sh a37fcba3c` still reports DIVERGED (2 commits: `a4ede5fa0`, `8fdd9ae56`). Label gap: bench/audit infrastructure has no own label, mapped to `tech-debt`.

- **Severity**: LOW
- **Dimension**: Telemetry & Origin Cost (bench discipline)
- **Location**: `scripts/fsr-bench-matrix.sh:131-146` (Prospector scene args), `scripts/fsr_bench_report.py`
- **Status**: NEW as a finding. The need for a re-bench is already tracked in ROADMAP R6a-stale-25, but no GitHub issue exists for either harness edit.
- **Description**: `scripts/check-bench-harness-provenance.sh a37fcba3c` now reports **DIVERGED** with two commits:
  - `8fdd9ae56` (#5128, 2026-09-30) appends `camera_pos`/`camera_forward` columns. Measurement-neutral by inspection (the baseline audit made the same call).
  - `a4ede5fa0` (#5257, 2026-10-06) adds `--bsa "Update.bsa"` and `--textures-bsa "Update.bsa"` to the `prospector` scene's argument list. That changes the archives the measured scene mounts: "overrides 36 entries" plus 2 textures per the commit message. The commit states entity count and `p0` source count are unchanged, but no frame-time control exists.
  Per the skill, an edit to either file that was not itself re-benched is a finding. The script's own comment asks that the change "land byte-identically across the next same-machine control".
- **Evidence**: provenance script output (quoted above); `git show a4ede5fa0 -- scripts/fsr-bench-matrix.sh`.
- **Impact**: any future comparison of a Prospector frame time against the 2026-09-28 record is not apples-to-apples until a same-machine control runs both sides. The record is already 455 commits behind HEAD, so this compounds an existing gap rather than creating a new one.
- **Related**: ROADMAP R6a-stale-25, R6a-regress-22, #5128, #5257.
- **Suggested Fix**: fold into the R6a-stale-25 refresh: rebuild `a37fcba3c` in a worktree with the *new* harness, bench both sides in one session, and record the harness commit alongside the new record.
