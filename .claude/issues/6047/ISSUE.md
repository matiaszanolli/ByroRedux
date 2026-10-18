# ESM-2026-10-18-EXT-12: a WRLD record without `EDID` leaves the previous worldspace current, so its children are filed under the wrong worldspace

Labels: low,esm-plugin,bug
Source: docs/audits/AUDIT_ESM_2026-10-18.md

- **Severity**: LOW (hostile or broken-mod input only; every vanilla WRLD has `EDID`).
- **Dimension**: CELL / WRLD Walkers
- **Location**: `cell/wrld.rs:242-264`. `current_wrld_name` is assigned only inside `if !record.editor_id.is_empty()`.
- **Status**: NEW
- **Description**: after an EDID-less WRLD, the following type-1 world-children group is parsed with the previous worldspace's name and merged into its cell map.
- **Suggested Fix**: on an empty `EDID`, set `current_wrld_name = None` (or key by FormID).

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix


_Filed from `docs/audits/AUDIT_ESM_2026-10-18.md` (`/audit-suite --area exterior`, HEAD 6f44874ac). Fix with `/fix-issue <number>`._
