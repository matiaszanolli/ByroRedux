# SPT-2026-11-06-D3-02 (#6723)

Snapshot of the issue as filed; GitHub is authoritative for state.

**Severity**: LOW · **Dimension**: TREE→Billboard Wiring (audit infrastructure) · **Status**: NEW
**Location**: `.claude/commands/audit-speedtree/SKILL.md:192` (`boot/schedule/late.rs`)
**Source**: `docs/audits/AUDIT_SPEEDTREE_2026-11-06.md`

## Description
#6703 (`22e0f71fc`) moved the Late-stage registrations into `boot/schedule/late/`. The billboard system's `Access` (`reads::<SpeedTreeWind>()` and the rest) now lives in `boot/schedule/late/camera.rs:70-79`. `late.rs` still exists as the hub, so `_audit-validate.sh` passes. A scoped `grep` or `git log` on `late.rs` sees only the hub, as `_audit-common.md` warns.

## Impact
The next run greps the wrong file and reports the declaration missing.

## Related
#6703.

## Suggested Fix
Cite `boot/schedule/late/camera.rs` in the skill (and name the hub-split trap if it recurs). Fold into the next `/audit-sync`.

## Completeness Checks
- [ ] **SIBLING**: Other audit skills citing `boot/schedule/late.rs` for a system's `Access` checked
