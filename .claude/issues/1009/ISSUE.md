# #1009 — C6-NEW-07: 300s per-client read timeout blinds shutdown signal

- **Severity**: LOW
- **Domain**: worker threads / debug server
- **Audit**: `docs/audits/AUDIT_CONCURRENCY_2026-05-13.md`
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/1009

## TL;DR
`set_read_timeout(300s)` means idle per-client threads observe shutdown only after 5 min or client disconnect. Currently absorbed by thread detachment; a future runtime change to `join_all` at exit would expose 5-min teardown latency.

## Fix
On listener shutdown, `shutdown(Shutdown::Both)` every active TCP stream. Requires `Vec<Arc<TcpStream>>` shared with listener, pruned on per-client thread exit. ~20 lines.
