# #6852: XCT Phase 1: relocation ledger, cross-space MoveTo, logical identities
State: OPEN
Labels: enhancement ai save-load 

Part of #6850. PRD [§9 Phase 1](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/cross-cell-actor-travel.md#9-delivery-sequence). Depends on M45 (saves).

## Acceptance Criteria
- [ ] XCT-DEST-001: a non-resident package destination resolves to its space and position through `EsmCellIndex::locate_refr`, not a wander target
- [ ] XCT-DEST-003, XCT-DEST-004: an unknown space, or a non-persistent actor, is **Unroutable** with its reason
- [ ] XCT-LOC-001, XCT-LOC-002: one FormID-keyed relocation ledger; the cell loader spawns a ledgered actor in its current space and not in its home cell
- [ ] XCT-LOC-003: the ledger is saved and restored (`FORMAT_MAJOR` bump, no `serde(default)`)
- [ ] XCT-LOC-004: a scripted `MoveTo` of an actor to a reference in another space relocates it, watched or not
- [ ] XCT-LOC-005: every persistent actor whose cell is not loaded has a logical identity (the `RemoteSceneActorStub` path extended)
- [ ] XCT-LOC-006: Papyrus `MoveToPackageLocation` places the actor at its package location across spaces
- [ ] XCT-ROUTE-007: a moved actor keeps inventory, equipment and script variables
- [ ] XCT-RPT-001: `actor.where <entity|0xREFR>` prints current space, pose and state
- [ ] §7.2 scenarios 2 (persistence, `cargo test`) and 3 (scripted move, FNV headless) pass

**Exit:** acceptance scenarios 2 and 3 pass.

