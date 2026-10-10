---
description: "Re-sync every audit skill against the code that landed since the last sync — closed-issue sweep, delta read, premise re-measure, owner-map fold-in"
argument-hint: "[--base <rev>]"
---

# Audit-Skill Sync

Audit skills describe the code: struct sizes, counts, test names, known-open issues. Fix commits change the code without touching those files, so the skills go stale. In every sync so far, most of the rot has been closed issues still described as open. This skill brings every `.claude/commands/audit-*/SKILL.md` and the shared `_audit-*` files back in line with HEAD. It does not audit the code and does not fix it: real bugs that surface are reported to the user, not fixed.

Files in this directory:
- `.claude/commands/audit-sync/prep.sh`: the deterministic preparation step.
- `brief.tmpl`: the instructions every agent reads.
- `groups.txt`: which agent owns which skills.
- `BASELINE`: the commit the last sync matched.

## Phase 1: Prep

```bash
SCRATCH=<session scratchpad>/audit-sync
.claude/commands/audit-sync/prep.sh "$SCRATCH"            # base = BASELINE; override with --base <rev>
```

The script writes the following into `$SCRATCH`:
- *BRIEF.md*
- *open.txt*
- *closed_since.tsv*
- *cited_closed_recent_map.txt*
- *weights.tsv*
- an empty *proposals.md*

It also prints each group's weight: the changed files routed to it through `_audit-owners.md`, and the cited issues closed since the base. It exits non-zero if `groups.txt` misses an `audit-*` directory or lists one twice. A newly added audit skill must go into a group first.

Look at the printed weights. If one group carries well over ~2× the median of either number, split it in `groups.txt` before fanning out. Ten agents was the right size for ~300 commits. Two skills' routed counts are inflated by churn, not code: `regression` (every `.claude/issues/` note) and `tech-debt` (the `docs/audits/` reports, other `docs/` edits, `.claude/commands/`). Discount those two before splitting.

## Phase 2: Fan out

Launch one background `general-purpose` agent per `groups.txt` line, all in a single message. Each prompt contains:
- an instruction to read `$SCRATCH/BRIEF.md` first and follow it exactly;
- the exact files the agent owns (`.claude/commands/audit-<skill>/SKILL.md` plus any helper scripts in that directory);
- the main delta paths for its skills (crates and `byroredux/src` modules from `_audit-owners.md`);
- any skill-specific hot spots: issues from `cited_closed_recent_map.txt`, or a session closeout in HISTORY.md that touched the area.

The `meta` group also owns the following, and should check that every shim's description still matches its skill:
- `_audit-common.md`, `_audit-owners.md`, `_audit-severity.md`, `_audit-validate.sh`, `_audit-route.sh`;
- the `.agents/skills/source-command-*` shims, which are thin pointers to the canonical command.

Agents must not start sub-agents. A nested agent's completion notice never reaches its parent (see `/audit-suite`, orchestration hazard).

## Phase 3: Collect

As each report arrives:
1. Append its "proposed edits to files I don't own", code gaps and fixed-but-open issues to `$SCRATCH/proposals.md`. Keep a heading per group.
2. Once that agent is finished, fold any proposal aimed at another agent's skill into that skill yourself. Never edit a file whose owning agent is still running. Check each claim against the code before you write it: an agent's proposal is a lead, not a fact.

After the `meta` agent finishes, fold in all the `_audit-owners.md` row proposals:
- Rows are first-match prefixes, so a specific row goes **above** the general row it narrows.
- Never drop an owner that the general row gave the path, unless the proposal says why.
- Check each new row with `printf '%s\n' <path> | .claude/commands/_audit-route.sh`.

## Phase 4: Verify

```bash
bash .claude/commands/_audit-validate.sh 2>&1 | grep -E 'FAIL|STALE|OK:'
git diff --stat -- .claude .agents
```

Skill-reading tests: rerun any whose matched text was touched.

| Test | Reads | Run |
|---|---|---|
| `gpu_material_size_claims` (4 tests) | every `.claude/commands` file | `cargo test -p byroredux-renderer --lib gpu_material_size_claims` |
| `the_audit_skill_does_not_claim_the_rt_integrity_chain_is_unread` | `audit-renderer` | bin crate, below |
| `documented_texture_role_list_matches_the_struct` | `audit-nifal`, `audit-fo4`, `_audit-common.md` | bin crate, below |

To re-derive the list: `git grep -n '\.claude/commands' -- '*.rs'`, keeping the non-comment hits. The `skin_offsets_hasher_tests.rs` guards pin `_audit-common.md`'s hot-path rule in code but never read the file.

The bin crate needs rustc ≥ 1.96 (wasmtime 49's MSRV since 15a6b1d2d; #4466 set the earlier 1.94 floor):
`TC=$(rustup which --toolchain 1.96.0 cargo); PATH="$(dirname "$TC"):$PATH" "$TC" test -p byroredux --bin byroredux -- <filter>`.

## Phase 5: Close

1. Write the HEAD that prep reported (`<short-hash> <YYYY-MM-DD>`) into `BASELINE`. That is the commit the agents synced against, not a later HEAD.
2. Report to the user:
   - one line per skill on what was wrong and what changed;
   - **real code gaps or bugs to file**, after checking each one yourself;
   - **issues fixed in code but still open**, with their fixing commits. A `Fix #A, #B` subject auto-closes only `#A`.
3. Commit only when the user asks: `docs(audit): re-sync audit skills against <base>..<head>`.
