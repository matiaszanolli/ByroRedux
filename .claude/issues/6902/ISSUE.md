# #6902 MUS verify: New Vegas audio-marker overlap, layer mixing, positive ANAM level, battle-set precedence, intro/outro, faction sets

Part of #6860, filed by Phase 0 (#6866). PRD [MUS-SRC-005, MUS-SRC-007](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/game-music.md#51-location-sources-mus-src), [MUS-SEL-003](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/game-music.md#52-playback-and-track-selection-mus-sel), [MUS-CMB-001, MUS-CMB-003](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/game-music.md#53-combat-mus-cmb), Q3.

Settled from the data (`FalloutNV.esm`, 2026-11-13; `docs/engine/music.md` §6): every one of the 572 audio markers carries an `XRDS` radius (402 at 5000 units), which is the area; a Location set's six boundary percentages (100 / 66.6 / 33.3 on most) are percentages of that radius; the markers' `MNAM` / `NNAM` "layer trigger" values are 0.33 / 0.67 on every marker, with `BNAM` "Use Controller Values" set on 427 of them; every controller's day and night starts are 64 and 255 (xEdit renders `value / 256` of a day: 06:00 and 23:54).

**Verify, against the original** (the GECK wiki's Media Set and Media Location Controller pages, past geckwiki.com's bot check, or by listening): which marker wins where two areas overlap; whether a set's Outer / Middle / Inner tracks mix or crossfade, and what the marker's layer triggers change; what a positive `MUSC.ANAM` magnitude does to the level (`musDocMitchellHouse` +6, `EndCredits` +3); which wins between a controller's Battle sets and the region's `RDSB`; when a Battle set's Intro and Outro play; how the faction relation picking Neutral / Ally / Friend / Enemy sets is computed (8 to 10 controllers).

**Engine defaults until then:** the nearest marker whose radius holds the player, by depth into its area; the layers mix, each fading over the set's layer crossfade time (`FNAM`); a positive `ANAM` loops at `-|ANAM|` dB; the controller's Battle sets before the region's; Intro on entering combat, Outro on leaving it; faction sets reported Unresolved.

## Comments
One item settled by #6904 (3981afb82): the faction relation picks the Neutral / Ally / Friend / Enemy list by the conditional faction's relation to the player (Nexus wiki *Fallout New Vegas Music*, 2014). Still open: NAM1's forced-list option, which no source decodes, and the other items above.
