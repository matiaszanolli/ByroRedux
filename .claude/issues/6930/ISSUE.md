# #6930 — INC-2026-11-14-18: OBSE `SetCellMusicType` overrides are neither saved nor reset on an in-session load

**Source**: `docs/audits/AUDIT_INCREMENTAL_2026-11-14.md` (INC-18). Verified against HEAD `77eb7e6c0` at publish time.

- **Severity**: LOW
- **Location**: `crates/scripting/src/obscript_more/extras.rs` (`CellMusicTypes`); `byroredux/src/save_io/registry_completeness_tests.rs` (NOT_SAVED entry)
- **Changed in**: commit `d86ae6634`

## Description
`CellMusicTypes` is allowlisted NOT_SAVED ("overrides last the session, as OBSE's own change to a cell does" — unverified against xOBSE), and nothing clears `overrides` on load. An in-session load keeps overrides made after the save, while a fresh process loads without them.

## Impact
Session-dependent music state after a load on a rarely used OBSE call.

## Related
#6889 (closed feature)

## Suggested Fix
Decide: clear it on load as session state (`discard_outgoing_session_requests` or the load-apply path), or add it to the save.

## Completeness Checks
- [ ] **SAVE**: If a saved type's shape changes, `serde_default_guard_tests.rs` baseline is refreshed / `FORMAT_MAJOR` bumped
- [ ] **TESTS**: A regression test pins this specific fix
