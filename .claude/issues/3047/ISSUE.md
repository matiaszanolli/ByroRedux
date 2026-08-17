# REN-DOC-02: _audit-common.md shader-include roster lists 9 of 12 headers

**Issue**: #3047
**Severity**: LOW
**Labels**: `low,tech-debt,documentation`
**Source report**: `docs/audits/AUDIT_RENDERER_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_RENDERER_2026-08-16.md`.

**Location**: `.claude/commands/_audit-common.md`, the `Shader Includes:` row

## Description

`_audit-common.md`'s shader-include roster lists **9 of the 12 live headers** in `crates/renderer/shaders/include/`.

## Impact

`_audit-common.md` is the shared layout reference every audit skill reads. An incomplete include roster means three headers are invisible to any audit that works from the layout map rather than the directory — including the shader-constant provenance checks that depend on knowing every header.

## Suggested Fix

Refresh the row against `ls crates/renderer/shaders/include/`. Consider whether the row should enumerate at all, given it has now drifted — a pointer to the directory may be more durable than a list.

## Related

- #2984 (TD9-2026-08-16-02 — the shader-include allow-list missing `presentation.frag`; same "hand-maintained shader list drifts" class)
- #3045 (REN-DOC-01 — audit-infrastructure drift in the sibling skill)

## Completeness Checks
- [ ] **COMPLETE**: All 12 headers listed, or the enumeration replaced by a directory pointer
- [ ] **SIBLING**: The `Shaders:` row (21 GLSL sources) checked for the same drift
- [ ] **PATH-GATE**: `_audit-validate.sh` still passes

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3047 --json state` when live state is needed.*
