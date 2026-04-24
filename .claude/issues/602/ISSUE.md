# #602: FO4-DIM6-07: LIGH power-state sub-records (XPWR / XNAM / XLRL) not captured

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/602
**Labels**: enhancement, low, legacy-compat, 

---

**From**: `docs/audits/AUDIT_FO4_2026-04-23.md` (Dim 6)
**Severity**: LOW
**Location**: `crates/plugin/src/esm/cell.rs:1382` `is_ligh` path parses MODL + LightData (radius, color, fov, fade) but not FO4-specific `XPWR` (powered-state), `XNAM` (wire-connection target), or `XLRL` (linked-lock-ref for breaker-panel state).

## Description

FO4 LIGH records carry an electrical-connection graph that drives light fixtures' on/off state:

- **XPWR** — powered-state FormID (references the circuit this light attaches to).
- **XNAM** — wire-connection target for settlement wiring placement.
- **XLRL** — linked-lock-ref, used to gate breaker-panel state.

## Impact

FO4 wired-circuit lights (Sanctuary settlement fuse boxes, Vault 111 main switch) render always-on. Battery rooms and "generator powered" light fixtures ignore their connection graph.

## Suggested Fix

Defer — requires a full settlement-circuit ECS system to consume. Pre-work: capture XPWR as a raw `Option<u32>` FormID on `LightData` for the day the circuit system lands.

## Completeness Checks

- [ ] **TESTS**: When circuit system lands, assert settlement circuit topology graph is connected.

## Related

- Depends on future settlement-circuit ECS system (not yet planned).
