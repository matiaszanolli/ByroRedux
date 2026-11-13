# #6853: XCT Phase 2: watched travel through one load door (VCG02 Sunny)
State: OPEN
Labels: enhancement ai game:fnv 

Part of #6850. PRD [§9 Phase 2](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/cross-cell-actor-travel.md#9-delivery-sequence). Depends on M42 (packages) and the NAVM pathfinder.

## Acceptance Criteria
- [ ] XCT-DEST-002: a cross-space destination puts a watched actor in **Routing** instead of walking at the destination's coordinates
- [ ] XCT-ROUTE-001: a **Routing** actor walks to a load door in its space whose `XTEL` leads into the destination's space
- [ ] XCT-ROUTE-002: reaching it, the actor takes the `XTEL` pose in the destination space and the hop is logged
- [ ] XCT-ROUTE-005, XCT-ROUTE-006: it spawns next frame in a loaded cell, or continues **Offscreen**
- [ ] XCT-ROUTE-008: the actor fades out and in at the door
- [ ] XCT-ROUTE-009: a hop into the destination's space makes it **Arrived**
- [ ] §7.2 scenario 1: from Sunny's line in the Prospector Saloon, `actor.where 0x00104E85` reports `in WastelandNV` at `VCG02SunnyOutsideMarkerREF` within 120 s, and `fnv-new-game.sh`'s shooting leg passes without its console stand-in

**Exit:** acceptance scenario 1 passes.

