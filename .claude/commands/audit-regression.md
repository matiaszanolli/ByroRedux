---
description: "Verify closed bug fixes haven't regressed — dynamically discovers and checks"
argument-hint: "--issues <N,N,N> --limit <N>"
---

# Regression Verification Audit

Verify that ALL previously fixed issues have not regressed. Dynamically discovers closed bug issues and verifies their fixes are still in place.

See `.claude/commands/_audit-common.md` for project layout, methodology, deduplication, context rules, and finding format.

## Parameters (from $ARGUMENTS)

- `--issues <N,N,N>`: Only verify specific issue numbers (e.g., `--issues 1,2,9`)
- `--limit <N>`: Maximum number of closed issues to verify (default: 50)
- `--label <label>`: Filter issues by label (default: `bug`)

## Step 1: Discover Fixed Issues

Fetch closed bug issues from GitHub:

```bash
gh issue list --repo matiaszanolli/ByroRedux --state closed --label bug --limit 50 --json number,title,body,closedAt,labels
```

If `--issues` is provided, fetch only those specific issues instead.

For each closed issue, extract:
- **Issue number and title**
- **File references** from the body (look for backtick-quoted paths like `crates/nif/...`)
- **Fix description** from the body (look for "Acceptance Criteria", fix commits)
- **Related issues** (look for `#NNNN` references)

## Step 2: Verify Each Fix

### Step 2a: Locate the Fix
1. Search for the issue number in commit messages: `git log --oneline --grep="#<NUMBER>"`
2. If found, check the diff: `git show <commit> --stat`
3. Read the referenced file(s) to confirm the fix is present

### Step 2b: Check for Regression Tests
1. Search for test files referencing the issue: `grep -r "<NUMBER>" crates/ --include="*.rs" -l`
2. Look for test names: `grep -r "test.*<keyword>" crates/ --include="*.rs"`
3. Record what tests exist

### Step 2c: Assign Status
- **PASS**: Fix code confirmed present + regression tests exist
- **PARTIAL**: Fix code confirmed present but NO regression tests
- **FAIL**: Fix code is missing or broken (REGRESSION DETECTED)
- **UNVERIFIABLE**: Cannot determine fix location from issue body

## Step 3: Special Checks

For ByroRedux-specific fragile areas:
- **Depth bias** (#16 and decal fixes): Verify bias values still applied for decal meshes
- **TLAS descriptor** (validation fixes): Verify write_tlas called at init for all frames
- **NiBoolInterpolator** (parse fix): Verify read_byte_bool not read_bool
- **Name collision** (#9): Verify root_entity scoping still in AnimationPlayer
- **XCLL parsing** (cell lighting): Verify byte offsets for directional rotation

## Output

Write to: **`docs/audits/AUDIT_REGRESSION_<TODAY>.md`**

### Per-Issue Format
```
## #<ISSUE>: <Title>
- **Status**: PASS | PARTIAL | FAIL | UNVERIFIABLE
- **Closed**: <date>
- **Fix commit**: <hash> (or "not found")
- **File checked**: `<path>:<line>`
- **Fix present**: Yes / No / Unknown
- **Tests exist**: Yes / No
- **Notes**: <concerns>
```

### Summary Table
```
| Issue | Title | Status | Fix Present | Tests |
|-------|-------|--------|-------------|-------|
```

For any **FAIL** status, suggest: `/audit-publish docs/audits/AUDIT_REGRESSION_<TODAY>.md`
