# #1156 Investigation

## Current state (2026-05-22)

- **1175** total `.claude/issues/<N>/` directories with an `ISSUE.md`
- **73** carry a `State: OPEN` or `Status: OPEN` (or bold variant) field
- **59** of those 73 are actually `CLOSED` on GitHub — the stale pool
- **14** of those 73 are legitimately still OPEN — correctly marked
- ~1102 of the 1175 dirs have no `State`/`Status` field at all (the field appears to have been dropped from newer audit-publish outputs already)

The "80 stale" number in the original issue (filed 2026-05-17) has naturally drifted down to 59 — partly via issues closing, partly via new ISSUE.md files being written without the State field.

## Three options (from the issue)

### Option A — Add a post-`gh issue close` sync hook
- Every `gh issue close <N>` triggers a local `Status: OPEN` → `Status: CLOSED` edit on `.claude/issues/<N>/ISSUE.md`
- Pros: keeps the local file authoritative for queries
- Cons: workflow tax on every close; the hook itself is new infrastructure to maintain; doesn't address the question of *why* we need local state when GitHub is authoritative

### Option B — Drop the `State` field from ISSUE.md template
- Always query GitHub for state via `gh issue view <N>`
- Pros: single source of truth (GitHub)
- Cons: still need to sweep the 59 stale files (remove the field); doesn't reflect the actual usage pattern (file is read for context, not for state)

### Option C — Immutable snapshot semantics
- Treat `.claude/issues/<N>/` as the snapshot of "issue as conceived at filing time"
- Document the convention; do nothing to the existing files
- New ISSUE.md outputs from `/fix-issue` flow already don't write the field
- Pros: zero code change, zero file churn, no workflow tax; matches actual usage
- Cons: requires teaching the convention; "OPEN" in a local file becomes a meaningless artifact (but is also no longer interpreted as authoritative)

## My recommendation

**Option C.** Three reasons:

1. The audit-publish flow already stopped writing the State field for newer issues — 1102 of 1175 directories have no such field. Codifying that as the intended convention costs nothing.
2. The 59 stale files are read for context (issue body, completeness checks, related issues) — not queried for state. The stale `OPEN` is essentially noise in a field nobody reads as authoritative.
3. Aligns with the CLAUDE.md global preference: "Don't add features... beyond what the task requires." Adding a sync hook (Option A) introduces ongoing workflow tax to fix something that has no downstream consumers.

The natural follow-up under Option C: add a one-line note to `_audit-common.md` or wherever the ISSUE.md convention is documented stating the immutable-snapshot policy. That single doc edit closes the loop.

## Cross-reference

- Today's `/audit-tech-debt` Dim 10 finding TD10-NEW-03 (about #1229 being fixed-in-code-but-tracker-OPEN) is sibling pattern — same recommendation applies: tracker hygiene is manual, not automated.
- `_audit-validate.sh` was the precedent that closed the *audit-skill path-drift* class via a structural gate. Option A would be the analogous gate for *ISSUE.md state drift*, but unlike paths-on-disk (verifiable via `test -e`), GitHub state requires network access — making the gate slow and brittle.
