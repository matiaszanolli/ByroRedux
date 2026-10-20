# TD1-2026-10-19-08: `nif/src/blocks/mod.rs::parse_block_inner` is a 1102-line, 255-arm `match type_name` (the largest match in the workspace)

Labels: low,nif-parser,nif,tech-debt,bug
Source: docs/audits/AUDIT_TECH_DEBT_2026-10-19.md

- **Severity**: LOW
- **Dimension**: 1 - File / Function / Module Complexity (match arms > 50)
- **Location**: `crates/nif/src/blocks/mod.rs:265` (fn spans L265-1366; ~289 arm-head lines incl. or-alternatives)
- **Status**: NEW to this report series (identical size at base; never filed)
- **Effort**: medium
- **Description**: nearly every arm is `"X" => Ok(Box::new(x::X::parse(stream)?))` interleaved with rationale comments, grouped by family only by comment. It is a registry written as control flow, and the live arm count is quoted in `CLAUDE.md`. Two tests scan this file's text (`import/collision/mod.rs:788`, `import/walk/lights.rs:264` via `include_str!("../../blocks/mod.rs")`), and `every_dispatched_bhk_shape_has_resolve_arm` (collision/mod.rs:858) depends on that scan.
- **Evidence**: `fnscan.py` row; `awk 'NR>=265 && NR<=1370' blocks/mod.rs | grep -cE '^        "[A-Za-z0-9_]+"( \| "[A-Za-z0-9_]+")* =>|^        \| "'` -> 289.
- **Proposed fix**: family dispatchers chained like the collision ones (`fn dispatch_node(..) -> Option<io::Result<Box<dyn NiObject>>>`, `dispatch_geometry`, `dispatch_shader`, `dispatch_collision`, `dispatch_anim`, `dispatch_particles`, ...), each in the matching `blocks/<family>.rs`, with `parse_block_inner` keeping the groupID prologue and trying families in order; or a `static BLOCK_PARSERS: &[(&str, ParseFn)]` sorted slice. Re-point the two `include_str!` scans (and the dispatch-count doc) in the same commit.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the related files named above
- [ ] **TESTS**: A regression test / source-scan guard pins this specific fix (or the finding is a pure move/delete covered by existing tests)

_Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-19.md` (HEAD 12677d718). Fix with `/fix-issue <number>`._
