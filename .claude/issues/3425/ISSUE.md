# #3425 — TD4-2026-08-27-01: the fix→issue link is only checked in one direction — a fix landing without a closing keyword leaves its issue OPEN forever (5 current orphans)

Labels: `medium,tech-debt,bug`
Filed: 2026-08-28 · Source report: `docs/audits/AUDIT_TECH_DEBT_2026-08-27.md`

---

**Severity**: MEDIUM · **Dimension**: 4 — Audit-Finding Rot · **Source**: `docs/audits/AUDIT_TECH_DEBT_2026-08-27.md` (TD4-2026-08-27-01)

**Location**: `scripts/check-issue-traceability.sh` (`--window` mode, lines ~36-104); orphaned fix sites at `crates/ui/src/avm2_host.rs`, `byroredux/src/main.rs`, `crates/renderer/src/vulkan/morph_compute.rs`, `crates/plugin/src/esm/records/misc/water.rs`

## Description
#3218 established that 43 of 134 issues closed in the 2026-08-16..20 window had no citing commit, and its fix added `--window` mode to `check-issue-traceability.sh`. Both of that script's modes take the **closed/declared** set as their input:

```bash
# PR mode — input is the PR body's declared closes
mapfile -t closing_issues < <(printf '%s\n' "${pr_body}" | closing_issue_numbers)
# --window mode — input is gh's closed-issue list
gh issue list --state closed --limit 500 --search "closed:>=${since%T*}" ...
```

An issue that was **fixed but never closed** is in neither input. It is not in a PR body (the script's own comment records that this repo's history is overwhelmingly direct commits to main, so that gate never fires), and by definition it is not in the closed set. It is therefore structurally invisible to the tool built to protect this linkage — and unlike the direction #3218 covered, this one does not merely lose archaeology: **the issue stays open, so the work is re-planned, re-audited, and can be re-implemented or reverted.**

The signal that makes it mechanically detectable is already in the tree: the fix author writes the issue number into the source comment.

## Evidence
Verified at publish time (2026-08-28):

```
$ for n in 3149 3151 3155 3244 3270; do gh issue view $n --json state -q .state; done
OPEN OPEN OPEN OPEN OPEN

$ grep -rn "#3270" crates/plugin/src/esm/records/misc/water.rs
1068:    // FO4's first float is the depth amount. Offsets 12/16 are not fog
1069:    // distances: across vanilla Fallout4.esm they are normalized values near
1070:    // 1.0, and treating them as distances collapses every ramp to ~1 BU
1071:    // (#3270). Keep the canonical 80/600 fog defaults until those fields'
1072:    // actual shader roles are identified.

$ git log --format="%h %ad %s" --date=short -1 -S "Offsets 12/16 are not fog" \
      -- crates/plugin/src/esm/records/misc/water.rs
98eea9b3 2026-08-25 Refactor exterior session reload and bootstrap mode handling
```

| Issue | Fix site (verified present) | Landing commit | Commit cites it? |
|---|---|---|---|
| #3149 | `crates/ui/src/avm2_host.rs` | `4e1afcbe` | no |
| #3151 | `crates/ui/src/avm2_host.rs` | `4e1afcbe` | no |
| #3155 | `byroredux/src/main.rs` | `4e1afcbe` | no |
| #3244 | `crates/renderer/src/vulkan/morph_compute.rs` (3 sites) + `context/draw.rs` | `98eea9b3` | no |
| #3270 | `crates/plugin/src/esm/records/misc/water.rs` | `98eea9b3` | no |

`4e1afcbe`'s body is nine Conventional-Commits sub-lines across 39 files and 927 insertions with zero `#NNNN` — and it is the commit that *introduced* `check-issue-traceability.sh`. Window-wide, **133 of 256 Rust-touching commits since 2026-08-20 carry no closing keyword** (162 of 303 including docs-only commits). The same omnibus shape recurs in `3aebf414`, whose message describes only a smoke-test fixture refactor while its diff also deletes fifteen `pack.rs` public functions.

## Impact
Two compounding, already-realised costs. (1) `/audit-regression`'s Step 2.1 is `git log --grep="#<N>"`; for these issues it returns nothing, so the audit cannot distinguish "no citation" from "no fix" — the script's own comment calls this degradation self-concealing. (2) More expensive: the issues remain OPEN, so they are re-triaged and re-investigated. Concurrent audits in the 2026-08-27 suite run independently rediscovered several of them, and a future `/fix-issue` on one could re-implement or revert live, correct code.

## Related
#3218 (CLOSED — the forward direction, whose fix is the script this finding extends); the `feedback_multi_issue_commit_close` memory note (the narrower instance of the same class: `Fix #A #B #C` auto-closes only `#A`); #3149, #3151, #3155, #3244, #3270 (the current orphans).

## Suggested Fix
Add a third mode to `check-issue-traceability.sh`: for every `#NNNN` appearing in the `.rs` diff of `base..head`, if `gh issue view` reports OPEN and no commit in the range cites it with a closing keyword, list it as a candidate orphan. Advisory-shaped like `--window` (some code refs are legitimately forward-looking, e.g. #3307/#3308) but it would have surfaced all five mechanically. Pair with a commit-hygiene rule: an omnibus squash must carry one closing keyword per issue its diff resolves. Then close the current orphans with a comment naming their landing commit.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the PR-mode half of the script, `/audit-regression`'s Step 2.1 recipe)
- [ ] **TESTS**: A regression test pins this specific fix
