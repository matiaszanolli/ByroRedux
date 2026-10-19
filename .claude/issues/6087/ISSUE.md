# SAVE-SD-04: The post-load ambient re-seat runs before `ScriptPackage` is overlaid, so a script-package winner is not reproduced

Labels: low,save-load,ai,bug

**Severity**: LOW · **Dimension**: Live Load-Apply · **Data-Loss Class**: none (derived procedure state is cleared and re-installed; spurious package fragments)
**Location**: `byroredux/src/save_io/live_load.rs:620` (re-seat) before `:638` (`apply_deltas`); `byroredux/src/npc_spawn/ai_package.rs:892-894`, `:1036-1040` (script package precedence), `:1088-1099` (changed-handover clears behavior)

## Description
#4815's contract is "re-seat the winner against the restored clock and quest state *before* the overlay, so the overlay lands on the saved winner". `ScriptPackage` (M47.3, `register_replacing_component`, overlaid) changes the winner in `ambient_ai_package_system` (`script_package_of`), but it is restored by the overlay after the re-seat. The re-seat therefore stamps the scheduled winner W; the overlay then restores the script package's `TravelState` / `Traveled` markers and the `ScriptPackage` slot on an actor whose `AmbientPackageRuntime` names W. At the next in-game minute the tick selects the script package, sees `changed`, calls `clear_ambient_behavior` (deleting what the overlay restored) and installs the behavior fresh, reporting `W -> script` package transitions (PF_ fragments) for an actor that was already on the script package. The test comment at `round_trip_tests.rs:1715-1719` ("persisting its cached winner is unnecessary") assumes winners depend only on the clock and quest state; the alias-injected overlays are re-derived after the re-seat too.

## Impact
A saved script-package actor loses its Travel progress markers within about a game minute of loading and fires an extra package-change fragment. Cosmetic-to-minor, hence LOW.

## Related
#4815, #5864, #6074 (GAME-SD-04: the alias-overlay half is the load-side face of that finding), #6072 (GAME-SD-02).

## Suggested Fix
Overlay `ScriptPackage` (a plain-data replacing column) before the re-seat, or run the re-seat after `apply_deltas` and re-assert the markers; pin with a round-trip test that includes a running script package.

Source: docs/audits/AUDIT_SAVE_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other per-placement components absent from the eviction row)
- [ ] **TESTS**: A regression test pins this specific fix
