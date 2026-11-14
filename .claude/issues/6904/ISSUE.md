# #6904 MUS follow-up: New Vegas faction-relation media sets (Goodsprings and 8 more areas play none)

Part of #6860, found by Phase 2 (#6883). PRD [Q3](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/game-music.md#10-decisions-required), [MUS-SRC-005](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/game-music.md#51-location-sources-mus-src).

Q3 deferred the Neutral / Ally / Friend / Enemy media-set lists on the premise that the controllers carrying them also carry a Location set, so those areas would "play their Location set regardless of standing". The census over `FalloutNV.esm` + 9 DLC (`music::census_tests`, 2026-11-14) finds 9 of the 689 audio markers under controllers with relation lists and **no** Location set. `musCtrlGoodsprings` (`0x0014E8AD`) is one: Neutral, Ally and Friend all name `0x0014E872`, Enemy names `0x0014E89A`, and its conditional faction is `0x00104C6E`. Those areas report Location **Unresolved** ("faction-relation sets only") and play nothing from the marker. This is why PRD scenario 6 moved from the Goodsprings marker `0x00152433` to Sloan's `0x0015D028`.

**Needs:** how the relation is computed (#6902), then playing the matching list; the Neutral set is the natural default where no faction relation applies.

**Engine behaviour until then:** `music.status` reports the area Unresolved, with the controller and marker (`docs/engine/music.md` §6).
