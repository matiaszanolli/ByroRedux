# TD9-2026-10-19-01: The lanes added to run the ignored tests (#5799 pins, #5854 golden) cannot execute — zero registered runners — so every `#[ignore]` still runs by hand (regression of #5799, #5854)

Labels: medium,test-gap,tech-debt,bug
Source: docs/audits/AUDIT_TECH_DEBT_2026-10-19.md

- **Severity**: MEDIUM (promotion: `#[ignore]`d tests guarding fixes of closed HIGH issues #5005/#5228/#5238/#5243/#5487/#5489 sit in no running lane). Regression of closed #5799 / #5854 / #5905 (each closed on a workflow that never runs).
- **Dimension**: 9 - Test Hygiene
- **Location**: `.github/workflows/real-data-gates.yml:1-6,281-283` (`closed-high-pins`, `runs-on: [self-hosted, linux, x64, byroredux-game-data]`, manual-only since `fde8feec2`), `.github/workflows/rt-correctness.yml:6-9,80-83` (`on: schedule` + `runs-on: [... byroredux-rt]`), `byroredux/tests/golden/cube_demo_60f.png`
- **Age**: `ceca6eaa8` (2026-10-15, #5799), `10f2f276b` (2026-10-15, #5854), `fde8feec2` (2026-10-17, schedule disabled on the data lane only)
- **Effort**: small (decision) / medium (runner)
- **Description**: `gh api repos/matiaszanolli/ByroRedux/actions/runners --jq .total_count` = `0`. `rt-correctness.yml` kept its nightly `schedule:`; its last 4 runs: 3 `cancelled` after 24 h in queue, 1 `queued` (39243043594) — nightly golden coverage claimed by `10f2f276b` has never produced a result, and queued runs accumulate. The `real-data-gates.yml` header and `docs/contributing.md:314-318` honestly say "run locally", but the 50-row `pins=(...)` array exists only inside the YAML, so the "local run" is a hand-copy of 50 lines, and nothing else automates it. Every other ignore (344 total: byroredux 97, plugin 94, nif 46, bsa 34, scripting 20, ...) is data/GPU gated; none is runnable on a hosted runner, so effective automated coverage of the ignored set is 0. `cube_demo_60f.png` was last regenerated 2026-09-15 (`b23c92e2f`); 121 commits have touched `crates/renderer/shaders` since, and no run could have noticed (the combustion golden was regenerated 10-15, 15 renderer commits since).
- **Evidence**: `gh run list --workflow rt-correctness.yml --limit 30` -> `cancelled/schedule x3, queued/schedule x1`; `git log --oneline $(git log -1 --format=%h -- byroredux/tests/golden/cube_demo_60f.png)..HEAD -- crates/renderer/shaders | wc -l` = 121.
- **Impact**: the closed-HIGH regression nets have only ever been exercised when an audit remembers the right `--ignored` filter; docs/skills (`audit-runtime`, `audit-nif`, `audit-scripting`) still describe these as "nightly".
- **Existing**: #5905 (closed, low; recorded the runner gap for the parsers lane only), #5799, #5854 (both closed).
- **Suggested Fix**: (1) move the pin list to a checked-in data file (`.github/real-data-pins.txt`) read by both the YAML and a new `scripts/run-real-data-pins.sh` (one process per pin, same refuse-the-no-op check) so the maintainer's local run is one command; (2) drop `schedule:` from `rt-correctness.yml` the way `fde8feec2` did for the data lane (or register the runner) so the queue stops filling; (3) have `/audit-runtime` run the golden pair + pins script and record the result; (4) regenerate or confirm `cube_demo_60f.png` on the dev box in the same pass.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the related files named above
- [ ] **TESTS**: A regression test / source-scan guard pins this specific fix (or the finding is a pure move/delete covered by existing tests)

_Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-19.md` (HEAD 12677d718). Fix with `/fix-issue <number>`._
