# REG-2026-08-20-PROC-01: 43 of 134 closed issues (32%) have no commit citing them

**Issue**: #3218 — https://github.com/matiaszanolli/ByroRedux/issues/3218
**Severity**: MEDIUM
**Labels**: `medium,tech-debt,bug`
**Source report**: `docs/audits/AUDIT_REGRESSION_2026-08-20.md`
**Filed**: 2026-08-20 · `/audit-publish` · verified against HEAD `bb0b92f2`

---

Filed from `docs/audits/AUDIT_REGRESSION_2026-08-20.md` § "Archaeology finding — the fix→issue link is broken for a third of the window."

**Severity**: MEDIUM · **Type**: defect in the verification chain, not in shipped code

## This is not a correctness problem. It is a *verifiability* problem — and it disables an entire audit type.

Of the **134** issues closed since 2026-08-16:

- **43 (32%) have no commit whose message cites them** — `git log --grep="#N"` returns empty
- **14 have no citation anywhere in the tree** — not in a commit, not in a code comment, not in a doc

**Every one of those 14 was hand-verified at HEAD this sweep, and all 14 are genuinely fixed.** So there is no code regression here. What is broken is the *link* that `/audit-regression` structurally depends on.

## The 14 zero-citation closures (all verified fixed)

| Issue | Verified fixed at | Closing commit |
|---|---|---|
| #2930 | `acceleration/blas_static.rs:566` `record_scratch_serialize_barrier` | — |
| #2938 | `core/character/derived.rs:74` `debug_assert!` | — |
| #2987 | `SKYRIM_HEALTH_ACTOR_VALUE` removed; `parse_real_esm.rs:191` pins `0x3E8` | — |
| #2988 | 5 / 5 VMAD sites now `parse_with_remap` | — |
| #2993 | `items.rs:404-408` — FO4 arm reads `value, weight, health` | — |
| #2994 | `items.rs:449` — `b"FNAM"` FO4 arm added | — |
| #2995 | `items.rs:521` — FO4 `AMMO DATA` 8-byte arm added | — |
| #3000 / #3001 / #3002 / #3003 / #3007 | `23068af0` "fix(smoke): make playable gates truthful" | `23068af0` (names none of the five) |
| #3023 / #3024 | `save/validate.rs:207-230` `EquippedWeapon` cross-check | — |
| #3026 | `InputAction::{Quicksave, Quickload}` (`interaction.rs:63-64`) | — |

## Why this disables the audit

`audit-regression/SKILL.md` Step 2.1 is `git log --oneline --grep="#<N>"`. For **a third of the window** that returns nothing. Step 2.3's grep fallback returns nothing for 14 of them.

The result is a report full of `UNVERIFIABLE` — or, worse and more likely, **a `FAIL` filed against a fix that is present**. That is not hypothetical: this same sweep came one command from publishing exactly such a false FAIL for an unrelated reason (see the `grep`-blindness finding filed alongside). The two failure modes compound: when discovery is unreliable, an auditor cannot tell "no citation" from "no fix."

## Two structural aggravators observed in this same delta

1. **`23068af0`** closed **five** issues in one commit while naming **none** of them.
2. **`73896726`** (*"Refactor water shader and related code for improved clarity and functionality"*) touched **30+ files across 8 crates** with a bullet-list body naming **no issue at all**.

Project memory already records the sibling hazard — *"Multi-issue Commit Close — `Fix #A #B #C` auto-closes ONLY `#A`"*. **This is the opposite failure of the same discipline**: there, the keyword is present but under-applied; here, it is absent entirely.

## Suggested Fix

1. **Require the `Fix #N` keyword per issue** in the commit that closes it. The repo convention already assumes this; it is simply not enforced. A `commit-msg` hook or a CI check on the PR body would make it mechanical.
2. **Where an issue is closed as a *side effect* of another fix** — #3102 via #3036, #3095's siblings via #2986 — **say so in the GitHub close comment**, so the archaeology survives the commit log. A one-line "resolved as a side effect of #NNNN" is enough.
3. Consider having `/audit-publish` or `/session-close` emit the current window's zero-citation list, so the gap is visible while the context is still fresh rather than discovered a sweep later.

## Impact

Every future `/audit-regression` run over this window inherits a 32% blind rate. The cost is borne entirely by that audit type, but it is the audit type whose whole purpose is proving that closed fixes stayed fixed — so the degradation is self-concealing: a regression audit that cannot find fixes reports lower confidence, not louder alarm.

## Related

- The `crates/plugin/src/esm/records/tests.rs` grep-blindness finding filed from this same report — the other half of "discovery, not code, is what failed this sweep"
- Project memory: *feedback_multi_issue_commit_close*
- `23068af0`, `73896726` — the two aggravating commits
- `audit-regression/SKILL.md` Steps 2.1 / 2.3 — the recipe this breaks

## Completeness Checks
- [ ] **ENFORCED**: The `Fix #N`-per-issue convention is mechanically checked, not just documented
- [ ] **SIDE-EFFECT-CLOSES**: A close comment convention exists for issues resolved by another issue's fix
- [ ] **VISIBLE**: The zero-citation set for a window is surfaced at close time, not discovered by the next regression audit
- [ ] **BACKFILL**: The 14 issues above carry a close comment naming where they were actually fixed, so the next sweep does not re-derive this table
