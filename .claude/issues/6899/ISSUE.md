# #6899 MUS verify: does PlayMusic mix over or replace the cell's music, end on a cell change, give way to battle

Part of #6860, filed by Phase 0 (#6867). PRD [MUS-SCR-005](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/game-music.md#55-script-requests-mus-scr), [MUS-CMB-004](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/game-music.md#53-combat-mus-cmb).

The only text is GECK *PlayMusic*'s note "-Will non stop the currently playing music of the cell." [sic], which reads either way. The data leans toward *replace*: New Vegas calls `PlayMusic 1NoMusic` (a music type with no file) in `VCG00` and in `VVault11ExterminationChairSCRIPT` (after `PlayMusic MUSVault10106`), and silencing with an empty type only has an effect if a scripted type takes the place of what is playing. The `VVault11` call is also explained by stopping its own scripted track, so it is not proof. A Fallout 3 caller to observe: `MQ11CitCraneTriggerScript` (`PlayMusic Tension`).

**Verify, against the original:** whether the cell's music keeps playing under a `PlayMusic` track; whether a cell change ends the scripted track; whether battle music takes over from it.

**Engine default until then:** the Script layer mixes over Battle and Location, a cell change ends it, and it does not give way to battle (PRD as approved; `docs/engine/music.md` §4).
