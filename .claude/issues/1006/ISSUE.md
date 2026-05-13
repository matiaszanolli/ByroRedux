# #1006 — C6-NEW-04: Screenshot bridge CLI vs debug-server race

- **Severity**: LOW
- **Domain**: worker threads / debug server
- **Audit**: `docs/audits/AUDIT_CONCURRENCY_2026-05-13.md`
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/1006

## TL;DR
`ScreenshotBridge` has 2 consumers (CLI `--screenshot` poll + debug-server `DebugDrainSystem`) but only 1 result slot. Last drainer wins the PNG, the other reports timeout.

## Fix
Preferred: route the CLI path through `DebugRequest::Screenshot`. Same flow, consolidates duplicated polling logic.

## Bundle with
#1011 (C6-NEW-09 — screenshot timeout leaves bridge.requested set; both stem from `ScreenshotBridge` not carrying request identity).
