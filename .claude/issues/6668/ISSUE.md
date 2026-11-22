# #6668: M48 Phase 6: Wave 2 (Fallout 4, FO3, Oblivion) and Starfield — AS3 extensions, menu events, every metric

Part of #6641. PRD [§9 Phase 6](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/game-ui-integration.md#9-delivery-sequence). Applies the machinery of Phases 1–4 (#6653, #6661, #6665, #6666) to the Wave 2 games and comes after Phase 5 (#6667); depends on M47.2 / M47.3 for the script events. Wave 2: Fallout 4 (AVM2), Fallout 3 and Oblivion (MenuXml); then Wave 3, Starfield, per Q6 and Phase 0's (#6643) Starfield verify. A*N* = §7.2 scenario *N*.

## Acceptance Criteria
- [ ] GUI-GFX-002: the AVM2 profile provides the AS3 extension surface (`scaleform.gfx.Extensions`) that vanilla Fallout 4 movies read, limited to the members Phase 0 measured (GUI-GFX-003)
- [ ] GUI-EVT-001: opening or closing a registry menu raises Fallout 4's `OnMenuOpenCloseEvent(asMenuName, abOpening)` on every script registered through `RegisterForMenuOpenCloseEvent` for that menu name (A8)
- [ ] GUI-EVT-002: the registration unregisters automatically when the owning quest stops or the active magic effect is removed
- [ ] Fallout 4: the behaviour, extension and text requirements Phase 3 met for Skyrim hold for the Fallout 4 profile (GUI-HOST-001…006, GUI-GFX-004, GUI-FONT-001…004, GUI-EVT-003/004), with an ignored corpus test of A7's shape finding no H4 in the vanilla archive's calls
- [ ] Oblivion: its MenuXml menus load and take input through the same `MenuProfile` machinery (GUI-XML-001…003)
- [ ] Fallout 4, Fallout 3, Oblivion: menus reach the surface the Phase 0 inventory assigns under Q1, native-page labels held to GUI-PAGE-005
- [ ] Starfield (Q6): if Phase 0 found a `BGSCodeObj`-shaped contract the AVM2 profile can extend, a Starfield profile and catalog and the rest of #4470's unbuilt bring-up (driver bars, protocol pin, `m48-8` smoke); otherwise Starfield recorded out of M48 with the reason
- [ ] A8: new `m48-menu-events.sh` (Fallout 4), indexed in `docs/smoke-tests/README.md`: the vanilla script Phase 0 chose receives one opening and one closing event (GUI-EVT-001)
- [ ] Every §8 metric (menu surface coverage, host behaviour coverage, placeholder share, HUD meter liveness, UI frame cost) published in its ROADMAP home for every in-scope game
- [ ] A1: the regression floor passes unchanged

**Exit:** scenario 8 passes, and every metric of §8 is published for every in-scope game.

