# #6667: M48 Phase 5: interactive race and sex selection menu

Part of #6641. PRD [§9 Phase 5](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/game-ui-integration.md#9-delivery-sequence). Depends on Phase 1 (#6653: the registry, `ui.open` / `ui.close`) and Phase 0 (#6643: the GUI-PAGE-002 caller-wait verify and the A9 fixtures); the surface follows Q1 / Q4, so it comes after Phase 4 (#6666). Face and body sliders are a separate milestone (GUI-NG-005). A*N* = §7.2 scenario *N*.

## Acceptance Criteria
- [ ] GUI-PAGE-002: `Game.ShowRaceMenu()` and ObScript `ShowRaceMenu` open an interactive race and sex selection (plus name where the game's menu has it, Q4) on the surface Q4 chooses, listed open in the registry until the player accepts; the auto-accept placeholder `Effect::ShowRaceMenu` no longer stands in (A9)
- [ ] GUI-PAGE-002: the calling script waits for the menu or carries on as Phase 0's verify found per game, and the quest state that verify named advances on accept (A9)
- [ ] GUI-PAGE-003: the Fallout 4 `ShowRaceMenu` parameters (`uiMode` 0–4, the menu target and vendor) lower; modes other than start-of-game chargen may report S4 until the slider milestone (GUI-NG-005)
- [ ] A9 (Skyrim SE): new `m48-race-menu.sh`, indexed in `docs/smoke-tests/README.md`, with MQ101 started at the stage whose fragment calls `Game.ShowRaceMenu()`: `ui.menus` lists `RaceSex Menu` open; `ui.close "RaceSex Menu" accept` closes it and the quest state advances (GUI-PAGE-002, GUI-OPS-005)
- [ ] A9 (FNV twin): the same at the first `ShowRaceMenu` call of the opening sequence located in Phase 0
- [ ] The MQ101 Known Issue closed in ROADMAP
- [ ] A1: the regression floor passes unchanged

**Exit:** scenario 9 passes on Skyrim SE and FNV, and the MQ101 Known Issue is closed in ROADMAP.

