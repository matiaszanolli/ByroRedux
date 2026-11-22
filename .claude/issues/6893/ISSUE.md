# #6893 — XCT verify: where the original places an actor that arrived offscreen

Part of #6850, filed by Phase 0 (#6851). PRD [XCT-OFF-002](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/cross-cell-actor-travel.md#53-offscreen-arrival-xct-off).

**Verify, in the original game:** an actor whose Travel crossed spaces while unwatched. When the player enters the destination space after its arrival, where does the actor stand: at its package location, at the far side of the load door it came through, or somewhere along the way?

**Engine default until then:** at the package location (the destination reference's pose), or at the far-side door when the player is already in the destination space at arrival (XCT-OFF-004) ([cross-cell-travel.md §4](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/engine/cross-cell-travel.md)).

Needs the original game.
