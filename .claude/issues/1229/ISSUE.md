# Issue #1229: TD7-NEW-02 / TD10-NEW-02 — 4 stale tri_shape.rs refs in audit skill files

**Status**: OPEN (creation snapshot — see #1156 for `.claude/issues/` semantics discussion)
**Severity**: LOW
**Labels**: low, tech-debt, documentation
**Source**: docs/audits/AUDIT_TECH_DEBT_2026-05-21.md
**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/1229

## Root cause

#1118 TD9-005 (a5aa5768, 2026-05-20) split `crates/nif/src/blocks/tri_shape.rs` (1875 LOC) into per-topic siblings:

- `crates/nif/src/blocks/tri_shape/ni_tri_shape.rs` — classic NiTriShape parser
- `crates/nif/src/blocks/tri_shape/bs_tri_shape.rs` — Skyrim SE+ packed-half BSTriShape parser
- `crates/nif/src/blocks/tri_shape/agd.rs` — NiAdditionalGeometryData

`tri_shape.rs` no longer exists; 4 audit skill files still reference it. `_audit-validate.sh` flags all 4 every run.

## Stale sites

| File | Line |
|------|-----:|
| .claude/commands/audit-fo4.md | 50 |
| .claude/commands/audit-renderer.md | 274 |
| .claude/commands/audit-skyrim.md | 56 |
| .claude/commands/audit-starfield.md | 68 |

## Fix

Sed sweep: replace ``` `crates/nif/src/blocks/tri_shape.rs` ``` with ``` `crates/nif/src/blocks/tri_shape/bs_tri_shape.rs` ``` in all 4 files. Update trailing "folded into the unified file post-Session-35" prose to "split out into tri_shape/bs_tri_shape.rs post-#1118 (2026-05-20)". Re-anchor `audit-renderer.md:277` from `~lines 665-730` to symbol-based (`BSTriShape` parser, packed-vertex loop) per post-#1040 convention.

## Verification

`bash .claude/commands/_audit-validate.sh` should report 0 STALE refs.

## Related

- Precedent: #1189 (post-#1115 render.rs path rot, closed via same sed-sweep pattern)
- Structural fix: #1114 (`_audit-validate.sh` gate)
- Sibling open: #1156 (local ISSUE.md drift class)
