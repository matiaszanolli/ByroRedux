# #1004 — REN-D3-NEW-01: failed_skin_slots HashSet retains despawned EntityIds

- **Severity**: LOW
- **Domain**: renderer / memory
- **Audit**: `docs/audits/AUDIT_CONCURRENCY_2026-05-13.md`
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/1004

## TL;DR
`failed_skin_slots: HashSet<EntityId>` is not cleared on cell unload — entries persist until the rare eviction-pass clear. Worse than the bytes leak: when an EntityId is recycled, the cached "failed" bit silently drops skin from a freshly-spawned NPC.

## Fix
In `unload_cell`: `ctx.failed_skin_slots.retain(|eid| !victim_set.contains(eid));`

## Bundle with
#1003 (same `unload_cell` hook site).
