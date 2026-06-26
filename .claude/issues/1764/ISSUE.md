# TD9-002: ZZZ_probe_ physics test ships dev-probe scaffolding (PROBE eprintln + ZZZ_ name)

_Filed 2026-06-26 as #1764 from docs/audits/AUDIT_TECH_DEBT_2026-06-26.md (immutable snapshot; query `gh issue view 1764` for live state)._

**Severity**: LOW · **Dimension**: 9 — Test Hygiene
**Location**: `crates/physics/src/water.rs:649` (`fn ZZZ_probe_buoyant_body_sleeps_and_sim_quiesces`) + `PROBE:` eprintln at `:683,687,691,694`
**Status**: NEW · **Audit**: TD9-002
**Note**: no `physics` domain label exists — mapped to `legacy-compat` per the audit-common subsystem table.

## Description
An otherwise-valid passing test (real `assert_eq!(ad, 0, …)` / `assert_eq!(steps, 0, …)` present) carries two development artifacts committed in `1645112ca` (2026-06-20): (1) a `ZZZ_probe_` name prefix (`ZZZ_` to sort it last, `probe_` marking a temporary investigation), and (2) three/four `eprintln!("PROBE: …")` diagnostics in the body. The `ZZZ_` ordering hack is unique in the suite.

## Impact
Cosmetic — noisy `PROBE:` lines in `--nocapture` runs; the `ZZZ_` name advertises a temporary probe as if permanent. No correctness risk.

## Suggested Fix
Rename to a descriptive name (e.g. `buoyant_body_sleeps_so_static_fast_path_re_engages`) and delete the four `eprintln!("PROBE: …")` lines. The two `assert_eq!`s already carry the test's intent.

## Completeness Checks
- [ ] **TESTS**: renamed test still passes; no `PROBE:`/`ZZZ_` left in committed code
