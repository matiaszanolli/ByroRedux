# #6851: XCT Phase 0: cross-space destination census, verify items and design (no engine code)
State: OPEN
Labels: enhancement ai 

Part of #6850. PRD [§9 Phase 0](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/cross-cell-actor-travel.md#9-delivery-sequence). No engine code.

## Acceptance Criteria
- [ ] §8 census: `crates/plugin/examples/cross_space_destination_census.rs` prints, per master, packages with a reference destination, cross-space destinations and the multi-hop share; its output for every in-scope master is in ROADMAP's M42 row
- [ ] Verify, XCT-ROUTE-001: which load door the original routes a watched actor through (VCG02 stage 10, Sunny leaving the Prospector Saloon), observed in the original game
- [ ] Verify, XCT-OFF-001: how long an unwatched Travel takes in the original (low process level)
- [ ] Verify, XCT-OFF-002: where the original places an actor that arrived offscreen when the player enters
- [ ] Verify, XCT-LOC-003: the FO3 / FNV / Fallout 4 / Starfield save-format equivalents of Oblivion's *Moved* / *Cell Changed* and Skyrim's `CHANGE_REFR_MOVE` / `CHANGE_REFR_CELL_CHANGED`
- [ ] `docs/engine/cross-cell-travel.md` written and added to the PRD's Related table

**Exit:** the census is in ROADMAP for every in-scope game, the design is merged, and every verify item is closed or filed as an issue.

