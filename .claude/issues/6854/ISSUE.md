# #6854: XCT Phase 3: offscreen arrival, multi-hop, followers, census sweep
State: OPEN
Labels: enhancement ai 

Part of #6850. PRD [§9 Phase 3](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/cross-cell-actor-travel.md#9-delivery-sequence).

## Acceptance Criteria
- [ ] XCT-OFF-001: an **Offscreen** actor arrives after its route's walk time (Q1)
- [ ] XCT-OFF-002, XCT-OFF-003: entering the destination space after that time shows the actor at its package location; before it, not yet
- [ ] XCT-OFF-004: a player already in the destination space sees the actor come through the far-side door at its arrival time
- [ ] XCT-ROUTE-003: multi-hop routes through each game's door links (FO3/FNV `NAVI` `NVCI`; Skyrim/Starfield `NAVM`; Oblivion door `XTEL`s)
- [ ] XCT-FOL-001: followers and escorts pass load doors with their leader (Q2)
- [ ] XCT-RPT-002: `actor.where --census` lists every ledgered actor
- [ ] §7.2 scenario 4: `docs/smoke-tests/xct-census.sh` passes per in-scope game (every destination **Arrived** or listed with its **Unroutable** reason)

**Exit:** scenario 4 passes and the arrival rate is published in ROADMAP for every in-scope game.

