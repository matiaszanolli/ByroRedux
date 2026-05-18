# TD10-001: 80 stale .claude/issues/<N>/ISSUE.md files mark issues OPEN while GitHub says CLOSED

## Source Audit
`docs/audits/AUDIT_TECH_DEBT_2026-05-17.md` — Dimension 10 (Audit-Finding Rot)

## Severity
**MEDIUM** — systemic operational-record drift, recurrent.

## Description
80 `.claude/issues/<N>/ISSUE.md` files declare `State: OPEN` (or `Status: OPEN`) while `gh issue view <N> --json state` returns `CLOSED`. Local issue files are created/edited during `/fix-issue` flow but never auto-synced when the GitHub issue is closed.

## Verified examples
- `#1076`: local says `State: OPEN`, GH state `CLOSED`
- `#1077`: local says `State: OPEN`, GH state `CLOSED`
- `#1135`: local says `**State**: OPEN`, GH state `CLOSED`

The same audit just confirmed 80 such cases across the `.claude/issues/` tree.

## Impact
Operational confusion. The `/fix-issue` skill could spuriously treat closed issues as actionable. Future audit agents have already had to spend tokens cross-checking via `gh issue view` per finding (every Dim 10 sweep re-discovers the same drift).

The `_audit-validate.sh` gate (added in #1114) closed the audit-skill path-drift class; this is the OTHER drift class that has metastasized.

## Decision pending — three options

**Option A** — Add a post-`gh issue close` sync hook that updates the local `Status` field. Workflow change; future-proofs the convention.

**Option B** — Deprecate the local `Status` field in `ISSUE.md` template. Always query GitHub for state. The local file becomes documentation/context, not authoritative state.

**Option C** — Treat `.claude/issues/<N>/` as immutable history (snapshot at issue creation, never updated). Local file reflects "issue as conceived"; GitHub is the source of truth for current state.

## Proposed Fix (after decision)
- **A**: Add the sync hook + sweep the 80 existing files to `Status: CLOSED`
- **B**: Edit `_audit-common.md` (or whichever template doc defines the format) to remove the `State` field; sweep the 80 files to delete the line; document the convention change
- **C**: No code change; just decide and document; the 80 files are "fine as-is" because they only reflect creation-time state

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Extend `_audit-validate.sh` to also check `.claude/issues/*/ISSUE.md` headers against GH state (only useful if we pick option A or B)
- [ ] **DROP**: N/A
- [ ] **TESTS**: N/A (process change)
