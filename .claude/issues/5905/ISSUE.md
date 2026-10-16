# #5905: PAR-D4-2026-10-15-05: every parser real-data suite's only automated lane has never produced a result, and nothing tracks it

- **Labels**: low,bug,import-pipeline,test-gap
- **Filed from**: `docs/audits/AUDIT_PARSERS_2026-10-15.md`

- **Severity**: LOW
- **Dimension**: Corpus Gates
- **Location**: `.github/workflows/real-data-gates.yml`:
  - the `corpus`, `parsers` and `closed-high-pins` jobs all have `runs-on: [self-hosted, linux, x64, byroredux-game-data]`;
  - the `parsers` loop is at `:208`.
- **Status**: NEW as an issue. The fact itself is recorded in the audit-parsers skill text ("runner count 0, 2026-10-15") with the advice to run locally. #4619 (closed) fixed the YAML that produced 0-job runs but not the missing runner. No open issue tracks the runner (search "self-hosted runner real-data-gates").
- **Trigger Input**: n/a.
- **Description**: the lane exists and is correctly shaped, but it is not a gate.
  - The workflow is well formed: `--no-fail-fast`, green-no-op refusal, strict env, and the LE `--skip` is named.
  - Every `--ignored` suite of the six parser crates is scheduled in it. game-detect has none, and its unit tests run in `ci.yml`.
  - The two FO4 HKX tests run there, since `BYROREDUX_FO4_DATA` is exported.
- **Evidence**:
  - `gh api repos/matiaszanolli/ByroRedux/actions/runners --jq .total_count` returns `0`.
  - The last 200 `real-data-gates.yml` runs: 184 `failure` (2026-09-15 → 2026-09-29, the pre-#4619 0-job era), 15 `cancelled` (2026-09-30 → 2026-10-14, queued and never picked up), and 1 `queued` (38711237512, 2026-10-15, at adbd468e2, which predates ceca6eaa8, so its job list has no `closed-HIGH real-data pins` job). **Zero successful runs, ever.**
- **Impact**: the "nightly" column of every parser gate is empty. All Dim 4 evidence comes from local runs, so the exact pins above guard nothing between audits.
- **Related**: #4619, #4659, #5799, #3919.
- **Suggested Fix**: either register a `byroredux-game-data` runner, or open a tracking issue and mark the lane "manual-only" in the workflow header and `docs/contributing.md`, so a cancelled run is not read as a gate.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the sibling producers/paths named in the finding
- [ ] **UNTRUSTED-INPUT**: The fix keeps every path bounded (no panic / OOB / unbounded alloc, recursion or CPU) on hostile `.pex`/`.psc`
- [ ] **TESTS**: A regression test pins this specific fix
