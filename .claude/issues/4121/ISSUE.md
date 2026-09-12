# SAFE-D11-02: _audit-common.md still describes extensions.rs as one file after #3843's 8-module split

**Issue**: https://github.com/matiaszanolli/ByroRedux/issues/4121
**Labels**: documentation, low, tech-debt, doc-rot

**Severity**: LOW (doc-rot, not a security gap)
**Dimension**: 11 - Sandboxed Mod Runtime Trust Boundary
**Location**: `.claude/commands/_audit-common.md:82` (the `extensions.rs (10,652 LOC, 24df5304 ...)` row); `.claude/commands/audit-safety/SKILL.md` Dimension 11 preamble; actual code now at `byroredux/src/extensions/{mod,install,systems,legacy_compat,tests,persist,capture,commands,dispatch}.rs`
**Status**: NEW (from `docs/audits/AUDIT_SAFETY_2026-09-11.md`)

## Description
`_audit-common.md`'s layout map — the shared path/line reference every audit skill is told to trust — still names `byroredux/src/extensions.rs` as a single file. Commit `55cbc0d6` ("Fix #3843: split extensions.rs into eight modules," 2026-09-11 12:24, closing #3843) turned it into a 9-file directory (8 production files + `tests.rs`) totaling 10,730 LOC, same `ExtensionHost` struct and behavior — file split only. `_audit-common.md` was itself touched again later the same day (commit `1efc5251`, 18:27, "sweep nine renderer doc-rot findings") without updating this row. This is the same class of gap as the previously-fixed `DOC-ROT-1`/`#3828` (which flagged the file as missing from the map entirely) — now the row exists but names a path that no longer resolves to a single file.

## Evidence
Confirmed live: `byroredux/src/extensions.rs` — absent (`ls`: "No such file or directory"). `byroredux/src/extensions/` — present, contains `capture.rs, commands.rs, dispatch.rs, install.rs, legacy_compat.rs, mod.rs, persist.rs, systems.rs, tests.rs` (9 files). `git merge-base --is-ancestor 55cbc0d6 1efc5251` → `yes`, confirming the split predates the doc file's last touch without being picked up. `.claude/commands/_audit-common.md:82` still reads `extensions.rs (10,652 LOC, ...)`.

## Impact
Documentation-accuracy only; no code defect. No behavior changed in the split.

## Related
#3828 (closed — the prior "missing from the map" state this supersedes), #3843 (closed — the split itself)

## Suggested Fix
Update the `_audit-common.md` layout-map row for `extensions.rs` to point at the `byroredux/src/extensions/` directory and its file list, matching the style already used for other split subsystems in the same table (e.g. `render/`, `cell_loader/`).

## Completeness Checks
- [ ] **SIBLING**: Row updated in the same style as other already-split subsystems in `_audit-common.md` (`render/`, `cell_loader/`)
