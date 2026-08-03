# NIFAL-D9-03: translation_completeness.rs fill-rate floors have ~33pp slack; metO/rghO/normal_map columns have no assertion at all

Source: `docs/audits/AUDIT_NIFAL_2026-08-03.md`

**Severity**: LOW
**Dimension**: Completeness · **Tier Violated**: (harness gap)
**Location**: `crates/nif/tests/translation_completeness.rs`
**Status**: NEW

## Description

Fill-rate floors in `translation_completeness.rs` carry ~33pp median slack
between the asserted floor and the measured value; `metO`/`rghO` (metalness/
roughness-override fill rate) are pinned at 100% with no assertion at all;
`normal_map` is asserted for no game. Sibling gaps in this same harness are
already tracked (`#2213` alphabetical-truncation confound, `#2214` raw-tier-
only measurement) — this finding covers the assertion-tightness gap itself,
not previously filed.

## Evidence

`crates/nif/tests/translation_completeness.rs:131-132` (`with_normal_map`
field), `:203` (the per-game report line format including `metO%`/`rghO%`/
`nrm%` columns), `:271-272` (`"metO%"`, `"rghO%"` header labels) — none of
these columns has a corresponding floor assertion in the test body.

## Impact

The harness measures these fill rates and prints them, but a regression that
drops `metO`/`rghO`/`normal_map` fill rate to zero for a game would not fail
`cargo test` — the completeness gate silently doesn't cover its own printed
columns.

## Suggested Fix

Add floor assertions for `metO`, `rghO`, and `normal_map` per game (mirroring
the existing floors for the columns that do have them), and tighten the
~33pp median slack on existing floors where the measured value has been
stable across sweeps.

## Completeness Checks
- [ ] **TESTS**: This finding's own fix is a test-harness change — the new assertions themselves are the regression coverage

## Filed as

GitHub issue #2307, labels: low, nif-parser, tech-debt, bug.
