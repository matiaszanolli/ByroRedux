# #3746 — TD8-2026-08-30-01: 73 committed `_tmp_*` scratch examples (6 978 LOC, 45 % of all example targets) link twice per CI run and are never linted — re-scope #3150 to the whole set

**Labels**: bug, medium, tech-debt

---

- **Severity**: MEDIUM
- **Dimension**: 8 — Dead Code & Backwards-Compat Cruft
- **Location**: `crates/nif/examples/_tmp_*` (58), `crates/sfmaterial/examples/_tmp_*` (10), `crates/plugin/examples/_tmp_*` (3), `crates/bsa/examples/_tmp_*` (1), `crates/facegen/examples/_tmp_*` (1); `.gitignore`; `.github/workflows/ci.yml`
- **Source**: `docs/audits/AUDIT_TECH_DEBT_2026-08-30.md` (`TD8-2026-08-30-01`), HEAD `64f64480`

## Relationship to #3150 — please re-scope, do not close this as a duplicate

**#3150 (OPEN, `ESM-2026-08-20-D4-01`) scopes the fix to the three `crates/plugin` probes**
and mentions "57 more sit in `crates/nif/examples/`" only as context. The real population
is **73 across five crates**, and #3150 carries none of the CI amplification evidence
below. **The recommendation is to re-scope #3150 to the whole set (or close it as
superseded by this issue) rather than work the two separately** — filing a near-duplicate
was the alternative and is worse.

## Description

`git ls-files | grep -E 'examples/_tmp_'` → **73 files** at HEAD: `crates/nif` 58,
`crates/sfmaterial` 10, `crates/plugin` 3, `crates/bsa` 1, `crates/facegen` 1.
**6 978 LOC — 45 % of all 164 committed example targets in the workspace.**

These are one-shot audit probes. Their own headers say so — e.g.
`crates/nif/examples/_tmp_sk_d1_part.rs:1`:
`//! TEMP: validate remap_bs_tri_shape_bone_indices' single-partition identity shortcut.`
Twenty-three carry a literal `//! TEMP scratch` banner.

**The convention they violate is the project's own.**
`docs/engine/exterior-readiness-plan.md:484` documents the intended pattern: a scratch
example is "`_tmp_land_stats.rs`, **deleted after use**". None of these 73 was deleted.
`.gitignore` has no `_tmp_` rule (verified), so **the default outcome of an audit session
is that its probes get committed.**

## Amplification — why this clears the LOW default

- `cargo test` builds example targets by default (to verify they compile) without running
  them.
- CI runs `cargo test --workspace` (`.github/workflows/ci.yml:92`) **plus a second
  `cargo test --workspace`** under the ABBA lock-order detector job.
- So every CI run and every local workspace test run compiles and links all 73 against
  `byroredux-nif`, `byroredux-bsa`, `byroredux-plugin`, `byroredux-sfmaterial` and
  `byroredux-facegen` — **73 extra link steps, twice per CI run, for zero coverage.**
- Meanwhile CI's clippy step is `cargo clippy --workspace` **without `--all-targets`**
  (`ci.yml:94`), so these 6 978 lines are **built but never linted** — they accrue lint
  debt invisibly.

**And it is still growing**: added 2026-08-03 (2), 08-07 (32), 08-08 (25), 08-12 (3),
08-16 (8), 08-29 (3). At the time of filing the working tree also carries **21 further
untracked `_tmp_a0830_*` files** from the current session — i.e. the next commit adds
another batch unless the `.gitignore` rule lands first.

## Suggested Fix

`git rm` the 73 files, then add `crates/*/examples/_tmp_*` to `.gitignore` so the
convention enforces itself. Any probe worth keeping should be promoted to a named,
documented example or folded into an `#[ignore]`d corpus test. Retention is cheap either
way — they stay in git history. Effort: small; **the highest compile-time payoff
available** this cycle.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — sweep for other un-ignored scratch prefixes before adding the rule
- [ ] **TESTS**: A regression test pins this specific fix — a CI check that `git ls-files` matches no `examples/_tmp_*` would make the convention self-enforcing
