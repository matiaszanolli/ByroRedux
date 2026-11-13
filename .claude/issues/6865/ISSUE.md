# MUS-0a: check in the music census and publish it in ROADMAP

State: OPEN · Labels: enhancement, esm-plugin, audio

Part of #6861 (#6860). PRD [§9 Phase 0](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/game-music.md#9-delivery-sequence).

The scratch record walker behind PRD §3.4 becomes a checked-in `crates/plugin` example, extended to the DLC masters. No engine code.

## Acceptance Criteria
- [ ] The example prints, per master, every music source: `CELL` `XCMT`/`XCMO`, `WRLD` `SNAM`/`ZNAM`, `REGN` `RDMD`/`RDMO`/`RDSB`/`RDSI`, `DOBJ` music slots, `MUSC`, `MSET`, `ALOC` and audio-marker `REFR`s, plus the `Data\Music` file count
- [ ] Its output for the base and DLC masters of Oblivion, Fallout 3 and New Vegas is in ROADMAP's per-game matrix (a music column); PRD §3.4 is replaced by a link to it
- [ ] §7.2 scenario 6's Goodsprings audio marker is named by FormID in the PRD
