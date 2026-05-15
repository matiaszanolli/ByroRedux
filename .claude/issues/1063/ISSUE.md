## Description

Of 90 actual `#[test] #[ignore]` annotations across the workspace, **25 hardcode `/mnt/data/SteamLibrary/steamapps/common/<game>/Data/<game>.esm`** in the test body. Even with `BYROREDUX_FNV_DATA` / `BYROREDUX_FO4_DATA` / etc. set, these tests will not pick up the env-var-pointed location — they hard-fail on a different machine.

The bigger consequence: **2 CRITICAL-closed and 4 HIGH-closed regression issues have their backing tests in this set.** That means a regression of `#405` (SCOL placements), `#533` (NAM0 weathers), `#754` (Starfield BSWeakReferenceNode dispatch), `#819` (FO4 parse-rate), `#934` (adaptive draw-sort), or `#965` (WRLD worldspace) would not be caught by `cargo test --release -- --ignored` on any system where game data isn't at the canonical Steam path.

The `data_dir("BYROREDUX_<GAME>_DATA", "/mnt/data/SteamLibrary/.../Data")` helper already exists at `crates/plugin/tests/parse_real_esm.rs:30` (proven working — it's how the `#819` parse-rate gate runs). The 25 hardcoded-path tests just need to be routed through it.

Lifted from Dim 6 of `AUDIT_TECH_DEBT_2026-05-14.md` (TD6-101..105, MEDIUM).

## Sites enumerated (25 tests across 5 files)

### `crates/plugin/src/esm/cell/tests/integration.rs` — 9 hardcoded
- L13 FNV.esm, L85+L171+L435 Oblivion.esm, L226+L487 Fallout3.esm, L255 Skyrim.esm, L342+L393 Fallout4.esm

### `crates/plugin/src/esm/records/tests.rs` — 4 hardcoded
(grep for `SteamLibrary` in the file)

### `crates/bsa/src/archive.rs` — 12 hardcoded
(real-data BSA-walk tests)

### `crates/bsa/tests/ba2_real.rs` — used for the FO4 / Starfield / FO76 ba2 walks
### `crates/plugin/tests/parse_real_esm.rs:30` — already has the `data_dir` helper (canonical proven implementation)

## Tests blocking closed CRITICAL/HIGH regression vectors

| Issue | Severity | Test | File:line |
|-------|----------|------|-----------|
| #405  | CRITICAL closed | `parse_real_fo4_esm_surfaces_scol_placements` | `cell/tests/integration.rs:341` |
| #533  | CRITICAL closed | `parse_real_*_esm_surfaces_nam0_weathers` (3 backing tests) | various |
| #754  | HIGH closed | `parse_starfield_bs_weak_reference_node_dispatch` | (cell tests) |
| #819  | HIGH closed | `parse_rate_fo4_esm` (uses env var) | `parse_real_esm.rs` ✅ |
| #934  | HIGH closed | adaptive draw-sort baseline | byroredux/tests |
| #965  | HIGH closed | `parse_real_*_esm_surfaces_wrld_worldspace` | `cell/tests/integration.rs` |

(All 5 still-broken tests `#[ignore]`d AND hardcoded.)

## Proposed fix

1. **Lift the `data_dir` helper out of `parse_real_esm.rs:30` into a shared `test-helpers` module.** Either a new `crates/byroredux-test-helpers/` crate (dev-dependency on every workspace member) or a `pub(crate)` module in `crates/plugin/src/test_helpers.rs` re-exported from elsewhere.

2. **Replace every `let path = "/mnt/data/SteamLibrary/.../GAME.esm"` site** with `let Some(path) = data_dir("BYROREDUX_<GAME>_DATA").map(|d| d.join("<game>.esm")) else { return; };` (or the same shape — early-return-or-skip when env var unset, so tests pass on CI without game data).

3. **Document the env-var contract in CLAUDE.md or a `crates/plugin/tests/README.md`** — there's currently no doc that says "set `BYROREDUX_FNV_DATA` to enable these tests".

4. **One-line CI gate** in `docs/smoke-tests/` or CONTRIBUTING.md: "tests requiring on-disk game data are `#[ignore]`d; set `BYROREDUX_<GAME>_DATA` and run `cargo test --release -- --ignored` to enable".

## Why this matters

The `#[ignore]` attribute is meant to gate "needs external resource" — it's the correct contract. But the *implementation* of the resource lookup is wrong in 25 of 90 cases: hardcoded paths instead of env-var-driven lookup means the contract is incomplete. A regression bisect that hits one of the 6 closed-CRITICAL/HIGH issues above would silently pass on a clean-data setup, only to fail much later in manual playtest. That's the worst possible failure mode for a regression guard.

The proven fix shape is in the repo already (`parse_real_esm.rs:30`); this is mechanical extension.

## Completeness Checks

- [ ] **UNSAFE**: N/A (test-only code)
- [ ] **SIBLING**: After fixing the 25 sites, audit the rest (`grep -rn "SteamLibrary" crates/`) — there may be a handful of `examples/` or `byroredux/tests/` sites with the same pattern that aren't `#[ignore]`d. Confirm zero residual hardcoded paths.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: After landing, run `BYROREDUX_FNV_DATA=$HOME/.local/data/FNV cargo test --release -p byroredux-plugin --release -- --ignored` (or similar) on a non-canonical path and confirm every test still runs.

## Effort
small (~1 h — extract helper to shared module + 25 site replacements + 2 doc additions)

## Cross-refs

- Audit report: `docs/audits/AUDIT_TECH_DEBT_2026-05-14.md` (Dim 6, TD6-101..107)
- #1050 (Test hygiene batch — this issue is the concrete actionable subset of that batch's "missing must-not-regress backing" theme)
- Closed-issue backing: #405, #533, #754, #819, #934, #965
- Proven helper: `crates/plugin/tests/parse_real_esm.rs:30` (`data_dir`)
