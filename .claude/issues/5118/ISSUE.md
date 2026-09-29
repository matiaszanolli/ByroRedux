# REG-2026-09-29-02: `check-playable-smoke-contracts.sh` does not neutralise `BYROREDUX_OBLIVION_DATA`; it runs the real P0 gate locally and then fails

**Labels**: medium,tech-debt,game:oblivion,bug

**Source report**: `docs/audits/AUDIT_REGRESSION_2026-09-29.md`

- **Severity**: MEDIUM
- **Dimension**: Guard integrity (smoke-gate contract lane)
- **Location**: `scripts/check-playable-smoke-contracts.sh:37-40` (env override block); `docs/smoke-tests/fixtures/oblivion.env` (`FIXTURE_GATES=(p0-door-interaction)`, `FIXTURE_DATA_DEFAULT=/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data`)
- **Status**: NEW. It is described as unverified in `.claude/commands/audit-runtime/SKILL.md` Dim 1 ("Verify against the script before filing"). This sweep verified it, empirically. No open issue covers it; #4547, which is closed, made the Oblivion route dispatchable but did not touch this script.
- **Description**: The contract loop runs each gate with the Skyrim SE / FNV / FO3 / FO4 data variables pointed at an empty temp dir, and expects exit 77 (SKIP). The Oblivion fixture (`84bbc44ed`, 2026-09-17) declares `p0-door-interaction` and reads `BYROREDUX_OBLIVION_DATA`, which the script never overrides. On any machine with Oblivion at the default path, the "missing-data" probe runs the real gate: `cargo run --release`, the engine, and `byro-dbg`. The gate then passes (exit 0), and the contract reports `FAIL -- p0-door-interaction[oblivion] missing-data path exited 0 instead of SKIP=77`. `set -e` plus `fail` aborts the script there, so every later contract never runs. That includes the #4730 literal-`run_gate` scan, the W1 route checks and the m47 self-test.
- **Evidence**: While verifying #4730, this sweep ran the script as the audit-runtime skill describes it ("runs each gate with data neutralised"):
  - It exited 1 with the FAIL line above.
  - `target/release/byroredux` was rebuilt at 17:39:59, which proves the real gate ran.
  - A re-run with `BYROREDUX_OBLIVION_DATA=<empty dir>` exited 0, and every remaining contract passed, including #4730's.
  - CI is not affected, because the runner has no Oblivion data.
- **Impact**:
  - The script is documented and CI-labelled as data-free, but on a normal dev machine it launches a GPU process. That breaks the project's "don't launch the engine beside the user's instance" rule, and it breaks the no-engine constraint of audit runs such as this one.
  - Every local run reads red.
  - The early abort hides the contracts that come after it.
- **Related**: #3039 (per-fixture SKIP≠PASS contract), #4547, #4730
- **Suggested Fix**: Build the override list from the fixtures, not a hand-kept list. For each `fixtures/*.env`, export its `FIXTURE_DATA_ENV` pointing at `$MISSING_DATA`. Add a self-check that fails when a fixture declares a data variable the loop does not neutralise.

**Validated at HEAD 9fcfdc3fc**: `scripts/check-playable-smoke-contracts.sh` overrides only `BYROREDUX_SKYRIMSE_DATA`, `_FNV_DATA`, `_FO3_DATA`, `_FO4_DATA`; `docs/smoke-tests/fixtures/oblivion.env` declares `FIXTURE_DATA_ENV=BYROREDUX_OBLIVION_DATA` and `FIXTURE_GATES=(p0-door-interaction)`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
