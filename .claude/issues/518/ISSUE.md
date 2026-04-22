# FNV-RUN-1: Console commands unreachable via byro-dbg

- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/518
- **Severity**: HIGH
- **Dimension**: Tooling / debug server
- **Audit**: `docs/audits/AUDIT_FNV_2026-04-21.md`
- **Status**: NEW (created 2026-04-21)

## Location

- `crates/debug-server/src/evaluator.rs:67` — `DebugRequest::Eval` falls directly to `eval_expr` without `CommandRegistry` lookup

## Summary

`tex.missing`, `tex.loaded`, `mesh.cache`, `mesh.info` (and every other registered console command beyond 4 hard-wired shorthands) fall through to the Papyrus expression parser when sent via `byro-dbg`. They parse as entity-member access → `no entity named 'tex'`.

Fix: pre-check `CommandRegistry` in `evaluator.rs:67` before falling through to `eval_expr`.

Fix with: `/fix-issue 518`
