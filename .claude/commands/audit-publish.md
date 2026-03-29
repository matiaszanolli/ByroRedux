---
description: "Convert an audit report's findings into GitHub issues"
argument-hint: "<path-to-audit-report>"
---

# Audit → GitHub Issues Publisher

## Process

1. **Load the report** at `$ARGUMENTS` (e.g. `docs/audits/AUDIT_RENDERER_2026-03-28.md`)

2. **Parse findings** — extract each finding block (ID, severity, location, description)

3. **Validate each finding** against current code:
   - Read the referenced file at the specified lines
   - Mark as: **CONFIRMED** (issue exists), **STALE** (already fixed), **UNVERIFIABLE** (can't confirm)
   - Skip STALE findings

4. **Deduplication check**:
   ```bash
   gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state
   ```
   Skip findings that match an existing OPEN issue title/description.

5. **Create GitHub issues** for each CONFIRMED + NEW finding:
   ```bash
   gh issue create --repo matiaszanolli/ByroRedux \
     --title "<ID>: <title>" \
     --body "<finding details>" \
     --label "<severity>,<domain>,bug"
   ```

6. **Summary** — print a table:
   | Finding | Action | Reason |
   |---------|--------|--------|
   | REN-001 | Created #42 | NEW, CONFIRMED |
   | REN-002 | Skipped | Existing #38 |
   | REN-003 | Skipped | STALE |
