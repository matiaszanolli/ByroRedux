# #1010 — C6-NEW-08: Unbounded CommandQueue<Vec<PendingCommand>>

- **Severity**: LOW (degenerate-client) / Theoretical
- **Domain**: worker threads / debug server
- **Audit**: `docs/audits/AUDIT_CONCURRENCY_2026-05-13.md`
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/1010

## TL;DR
`CommandQueue: Arc<Mutex<Vec<PendingCommand>>>` — per-client backpressure is 1-in-flight, but the Vec is unbounded across clients. Loopback-only (#857) so real-world risk is low; a CLI bug firing commands in a tight loop with `--bench-hold` would balloon memory.

## Fix
`crossbeam_channel::bounded(64)` or fixed-cap circular buffer. On overflow: synchronous `DebugResponse::error("server overloaded")`. ~15 lines.
