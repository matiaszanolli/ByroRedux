# #1011 — C6-NEW-09: Screenshot timeout leaves bridge.requested set — next request reads stale bytes

- **Severity**: LOW
- **Domain**: worker threads / debug server
- **Audit**: `docs/audits/AUDIT_CONCURRENCY_2026-05-13.md`
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/1011

## TL;DR
If drain system times out (>10 frames), `pending_screenshot = None` clears engine-side bookkeeping but `bridge.requested` may still be set. Renderer later writes a result nobody waits for — the next debug-server screenshot command picks up those stale bytes.

## Fix
On timeout, also `bridge.requested.store(false, Release)` and `bridge.result.lock().take()`. Two lines.

## Bundle with
#1006 (CLI vs debug-server race — both stem from `ScreenshotBridge` not carrying request identity).
