---
description: "Convert an audit report's findings into GitHub issues with completeness checks"
argument-hint: "<path-to-audit-report>"
---

# Audit → GitHub Issues Publisher

## Process

1. **Load the report** at `$ARGUMENTS` (e.g. `docs/audits/AUDIT_RENDERER_2026-04-04.md`)

2. **Parse findings** — extract each finding block (ID, severity, location, description, status)

3. **Filter** — only process findings with status **NEW**. Skip Existing/Regression (already tracked).

4. **Validate each finding** against current code:
   - Read the referenced file at the specified lines
   - Mark as: **CONFIRMED** (issue exists), **STALE** (already fixed), **UNVERIFIABLE** (can't confirm)
   - Skip STALE findings

5. **Deduplication check**:
   ```bash
   gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state
   ```
   Skip findings that match an existing OPEN issue title/description.

6. **Completeness checks** — for each CONFIRMED finding, generate checkboxes:

   ```markdown
   ## Completeness Checks
   - [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant
   - [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
   - [ ] **DROP**: If Vulkan objects change, verify Drop impl still correct
   - [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId ordering
   - [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes
   - [ ] **TESTS**: Regression test added for this specific fix
   ```

7. **Create GitHub issues** for each CONFIRMED + NEW finding:
   ```bash
   gh issue create --repo matiaszanolli/ByroRedux \
     --title "<ID>: <title>" \
     --body "<finding details + completeness checks>" \
     --label "<severity>,<domain>,bug"
   ```

   **Audit-type label override**: when the report path is `AUDIT_TECH_DEBT_*.md`, append the `tech-debt` label and use `maintenance` instead of `bug` as the type label (tech debt isn't a bug). Final label set: `<severity>,<domain>,tech-debt,maintenance`. Same pattern applies to any future audit-type label (e.g., a hypothetical `AUDIT_DOCS_*.md` would get a `docs` label + `documentation` type).

8. **Save to local tracking**:
   ```bash
   mkdir -p .claude/issues/<NUMBER>
   ```
   Write `ISSUE.md` with the finding details.

9. **Summary** — print a table:
   | Finding | Action | Reason |
   |---------|--------|--------|
   | REN-001 | Created #42 | NEW, CONFIRMED |
   | REN-002 | Skipped | Existing #38 |
   | REN-003 | Skipped | STALE |
   | NIF-004 | Created #43 | NEW, CONFIRMED |

10. **Suggest next step**: For each created issue, note:
    ```
    Fix with: /fix-issue <number>
    ```
