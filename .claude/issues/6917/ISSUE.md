# #6917 — INC-2026-11-14-02: An actor with both a relocation placement and a journey gets two logical-identity stubs

**Source**: `docs/audits/AUDIT_INCREMENTAL_2026-11-14.md` (INC-2). Verified against HEAD `77eb7e6c0` at publish time.

- **Severity**: HIGH
- **Location**: `byroredux/src/cell_loader/relocated.rs` (`reconcile_logical_identities`, the `moved` list); `crates/scripting/src/created_references.rs` (`ActorRelocations::relocate` / `start_journey`)
- **Changed in**: `relocated.rs` (commit `4b71155ca`)

## Description
`relocate` removes the actor's journey, but `start_journey` does not remove its placement, so any multi-hop route leaves the actor in both maps (hop 1 lands in a loaded cell → `relocate`; hop 2 or an unload → `continue_hop` / `depart_unwatched` → `start_journey`).

`moved` chains `ledger.iter()` and `ledger.journeys()` with no de-duplication, so the form is pushed to `wanted` twice. The spawn loop checks the `stubs` / `carried` / `live` snapshots taken before the loop, so the stub spawned for the first entry is invisible to the second, and a second `RemoteSceneActorStub` is spawned.

## Evidence
- `let moved: Vec<u32> = ledger.iter().map(|(actor, _)| actor).chain(ledger.journeys().map(|(actor, _)| actor)).collect();`
- `pub fn start_journey(&mut self, actor: u32, journey: Journey) { self.journeys.insert(actor, journey); }`

## Impact
Two entities carry the same `reference_form_id` and script set, so alias fills and scripts run twice. `identity_stubs` (keyed by form) tracks one; `promote` and the retire-beside-a-body step despawn only that one, and the other leaks for the session.

## Related
XCT-LOC-005

## Suggested Fix
`moved.sort_unstable(); moved.dedup();`; make `start_journey` and `relocate` keep the two maps mutually exclusive; add a placement-plus-journey reconcile test (`reconcile_logical_identities` has none today).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other readers that chain `iter()` and `journeys()`)
- [ ] **SAVE**: If a saved type's shape changes, `serde_default_guard_tests.rs` baseline is refreshed / `FORMAT_MAJOR` bumped
- [ ] **TESTS**: A regression test pins this specific fix
