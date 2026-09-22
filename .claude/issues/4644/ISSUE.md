# ESM-2026-09-21-D1-02: #3721's parent-end clamp covers only the recurse arm — every skip arm still seeks by the child's raw declared size

**Issue**: #4644
**Filed**: 2026-09-22 (audit-publish, AUDIT_ESM_2026-09-21.md)

**Severity**: LOW
**Dimension**: Header & GRUP Walk
**Location**: `crates/plugin/src/esm/reader.rs:947-949` (`skip_group`), `:1011-1017` (depth-cap arm of `bounded_group_content_end`); skip arms at `crates/plugin/src/esm/cell/walkers.rs:196,200`, `crates/plugin/src/esm/cell/wrld.rs:57,61,347,351`, `crates/plugin/src/esm/records/grup_walker.rs:249`

## Description
`parse_cell_group_inner`, `parse_wrld_group` and `parse_wrld_children_inner` compute a clamped `sub_end` but their `_ =>`/"no current cell" arms discard it and call `reader.skip_group(&sub_group)`, which advances by unclamped `total_size - header_size`. The depth-cap arm inside `bounded_group_content_end` has the same shape.

## Evidence
Sibling residual of closed #3721/#4076, which clamped only the recursive-call arm.

## Impact
Same LOW class as #3721 — not memory-unsafe, no vanilla master triggers it, but a crafted child GRUP landing in a skip arm can move the cursor past its parent's end, typically failing the whole plugin parse.

## Suggested Fix
Skip to the already-computed clamped `sub_end` (e.g. a `seek_to` helper) at all four sites; clamp the depth-cap skip to `parent_end`; add an overrunning-skip-arm fixture.

## Related
#3721, #4076 (closed)

## Source
docs/audits/AUDIT_ESM_2026-09-21.md (ESM-2026-09-21-D1-02)
