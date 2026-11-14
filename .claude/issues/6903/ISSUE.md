MUS verify: Oblivion music precedence, fade, battle recovery, StreamMusic across a cell change

Part of #6860, filed by Phase 0 (#6868). PRD [MUS-SRC-008](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/game-music.md#51-location-sources-mus-src), [MUS-SEL-004](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/game-music.md#52-playback-and-track-selection-mus-sel), [MUS-CMB-002](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/game-music.md#53-combat-mus-cmb), [MUS-SCR-003](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/game-music.md#55-script-requests-mus-scr).

`Oblivion.exe` names the four category folders, the two stings (`.\\Data\\Music\\Special\\death.mp3`, `success.mp3`) and `ExtraCellMusicType`, and has no music fade or combat-music setting (its only music settings are `fDefaultMusicVolume`, `fMainMenuMusicVolume`, `bMusicEnabled`, `sMainMenuMusic`). CS *StreamMusic* says only that "at the end of the file, a new random music will be chosen according to the current location".

**Verify, against the original:** the order cell `XCMT` → worldspace `SNAM` → region `RDMD`, by a Default cell inside a region with a non-Default `RDMD`; the crossfade between two places' music; how long battle music holds after combat; whether a cell change ends a streamed track.

**Engine defaults until then:** that order; the interim 3 s crossfade (`REGN_AMBIENT_CROSSFADE_SECS` at HEAD); Fallout 3's `fCombatMusicStopTime` (5 s); a streamed track ends on a cell change, like `PlayMusic` (`docs/engine/music.md` §3, §7).
