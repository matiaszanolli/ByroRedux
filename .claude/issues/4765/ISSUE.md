# TD8-2026-09-22-01: cargo clippy --workspace -- -D warnings red — 3 errors, all fresh, all in the byroredux bin crate

**Labels**: medium, tech-debt, bug

Filed via /audit-publish from docs/audits/AUDIT_TECH_DEBT_2026-09-22.md.

**Severity**: MEDIUM · **Dimension**: 8 — Dead Code & Backwards-Compat Cruft
**Location**: `byroredux/src/hud.rs:689` (`clippy::question_mark`); `byroredux/src/render/groundcover.rs:242` (`clippy::derivable_impls`); `byroredux/src/render/groundcover.rs:350` (`clippy::needless_borrow`)

**Status**: NEW
**Verified against**: HEAD `c3f298a24` — re-ran `cargo clippy -p byroredux -j4 -- -D warnings` live; all 3 errors present at the exact reported locations. No commit in `ee6d3fb39..c3f298a24` touched either file.

## Description

The exact CI gate command (`cargo clippy -p byroredux -j4 -- -D warnings`, no `--all-targets`) fails with 3 errors, all introduced by this delta's own fix commits: `66471d4553` (Fix #4608, HUD refresh staging) added the question-mark-eligible block; `c010c9fb96` (Fix #4607, ground-cover persistent scratch) both added the hand-written `impl Default` that clippy wants derived, and — by turning `candidates` into `&mut scratch.candidates` — made a three-week-old `&candidates` call site newly a needless double-reference.

## Impact

Any contributor on the CI toolchain (1.96.0) gets a red gate on code they didn't touch — same impact class as TD8-2026-09-21-01 (#4567), now recurring a second consecutive day from same-day fix commits rather than a toolchain bump.

## Related

#4607, #4608 (introduced these); #4567 / TD8-2026-09-21-01 (same gate class, prior cycle, independently occurring — not a regression of that fix, different lines).

## Suggested Fix

Apply clippy's own suggested rewrite at each site. Trivial ×3.

Source: docs/audits/AUDIT_TECH_DEBT_2026-09-22.md (TD8-2026-09-22-01)
