# STRM-N2 / #803 — Cloud scroll resets on every interior→exterior re-entry

**Severity:** LOW
**Domain:** binary (scene state machine)
**Audit:** `docs/audits/AUDIT_RENDERER_2026-05-03_EXTERIOR.md`

## One-line
`unload_cell` removes `SkyParamsRes`; `apply_worldspace_weather` rebuilds it with `cloud_scroll: [0, 0]` for all 4 layers — clouds visibly snap back to origin on every interior↔exterior transition. `GameTimeRes` survives correctly because it's never removed.

## Fix
Move the 4 cloud_scroll accumulators to a separate `CloudSimState` resource that survives cell transitions (Option 2 in the issue body, mirrors `GameTimeRes` pattern).
