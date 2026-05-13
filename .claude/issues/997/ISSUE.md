# #997 — SPT-D2-01: No "first wins" duplicate-tag semantics documented

- **Severity**: LOW
- **Domain**: documentation / legacy-compat
- **Audit**: `docs/audits/AUDIT_SPEEDTREE_2026-05-13.md`
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/997

## TL;DR
`import_spt_scene` collapses `scene.leaf_textures()` with `.first()` — implicit "first wins" with no inline comment. A future swap to `.last()` would silently change rendering.

## Fix
Add a one-line comment at the `.first()` call site. Optionally add a test exercising dup-tag selection.
