# #1007 — C6-NEW-05: pending_screenshot orphans renderer slot on recv_timeout mismatch

- **Severity**: LOW
- **Domain**: worker threads / debug server
- **Audit**: `docs/audits/AUDIT_CONCURRENCY_2026-05-13.md`
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/1007

## TL;DR
Per-client `recv_timeout(5s)` vs engine's 10-frame ceiling — when engine is paused, client times out + drops receiver. Drain system's `send` returns Err which `let _ =` swallows; file still gets written; client thinks it failed and re-issues. Multiple PNGs accumulate.

## Fix
`DebugDrainSystem::run` checks `pending.response_tx.is_closed()` via `crossbeam::channel`'s `is_disconnected()`; cancels capture if client walked away. Renderer slot returns to pool.
