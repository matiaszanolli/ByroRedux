# #1008 — C6-NEW-06: handle_client .expect() panics silently kill per-client threads

- **Severity**: LOW
- **Domain**: worker threads / debug server
- **Audit**: `docs/audits/AUDIT_CONCURRENCY_2026-05-13.md`
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/1008

## TL;DR
`set_nonblocking(false).expect(...)` and `try_clone().expect(...)` at listener.rs:139-144 panic silently with no `log::error!`. Brittle on FD exhaustion / socket-level errors.

## Fix
Replace `.expect()` with `match ... { Err(e) => { log::warn!(...); return; } }`. Three lines. Mirrors `cell_pre_parse_worker`'s recovery pattern.
