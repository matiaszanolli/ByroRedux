# #6666: M48 Phase 4: Wave 1 (Skyrim, FNV) — menu surfaces per Q1, dialogue menu first

Part of #6641. PRD [§9 Phase 4](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/game-ui-integration.md#9-delivery-sequence). Depends on Phase 3 (#6665: under Q1 a menu runs on its own movie or document only once its host calls are all served, H1), on Phase 1 (#6653, the registry) and on M43's dialogue layers (closed). Wave 1: Skyrim and FNV. A*N* = §7.2 scenario *N*.

## Acceptance Criteria
- [ ] GUI-PAGE-004: the dialogue menu reaches the surface Q1 chooses, first, keeping every behaviour the native page carries today (speaking cue, subtitle, replies, said-markers, [Dialogue Trees §3](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/engine/dialogue-trees.md#3-scope))
- [ ] GUI-PAGE-004 (FNV): `dt1-dialogue-layers.sh` Phase P passes on that surface (A1 runs the same script)
- [ ] GUI-PAGE-004 (Skyrim): the Phase P leg (speaker named, list mode, speaking cue) is added to `dt2-skyrim-forcegreet.sh` and passes on that surface
- [ ] Q1 (Skyrim, FNV): every other menu the Phase 0 inventory assigns S1 / S2 (its host calls all H1) runs on the game's own movie or document; the rest keep the native page (S3) as the fallback and are reported with their surface in `ui.menus`
- [ ] GUI-PAGE-005: every label a remaining native page shows resolves from the game's data (a record's name or a `GMST` string) unless the Phase 0 inventory lists it as an engine string with its reason; a per-page test asserts that no unlisted engine string is drawn
- [ ] GUI-PAGE-006 (SHOULD): the player-facing native pages move out of the operator overlay crate `byroredux-debug-ui` (`crates/debug-ui/src/panels.rs`)
- [ ] Menu surface coverage published for Skyrim and FNV in the compatibility matrix (§8)
- [ ] A1: the regression floor passes unchanged

**Exit:** Phase P passes on the chosen surface on both games (`dt1-dialogue-layers.sh` on FNV, the new Phase P leg of `dt2-skyrim-forcegreet.sh` on Skyrim, GUI-PAGE-004), and menu surface coverage is published for both.

