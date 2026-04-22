# FNV-RUN-4: CLI lacks --radius N

- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/531
- **Severity**: LOW
- **Dimension**: CLI / UX
- **Audit**: `docs/audits/AUDIT_FNV_2026-04-21.md`
- **Status**: NEW (created 2026-04-21)

## Location

`byroredux/src/scene.rs:124-133` — literal `3` in `load_exterior_cells(...)`

## Summary

audit-fnv spec documents `--radius 3` but the engine ignores it; grid radius is hardcoded. LOD/streaming audits that want smaller or larger grids have to patch source.

Fix: add `--radius N` parser, clamp `1..=7`, pass through to `load_exterior_cells`.

Fix with: `/fix-issue 531`
