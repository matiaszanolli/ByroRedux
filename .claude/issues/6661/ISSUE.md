# #6661: M48 Phase 2: path service and live vanilla HUD meters (Skyrim, Fallout 4)

Part of #6641. PRD [§9 Phase 2](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/game-ui-integration.md#9-delivery-sequence). Depends on Phase 1 (#6653: the registry the path service addresses open movies through) and Phase 0 (#6643: the design's AVM1 `_root` answer, the GUI-HUD-003 feed-path list). The extender `UI` functions GUI-PATH-005 routes also wait on the Legacy Mod Compatibility PRD's Phase 3 and the SDK plan's UI contribution service. Skyrim SE (AVM1) and Fallout 4 (AVM2). A*N* = §7.2 scenario *N*.

## Acceptance Criteria
- [ ] GUI-PATH-001: path-addressed get, set and invoke on any open movie of either VM through adapter injection, an AVM1 adapter mirroring the shipped AVM2 one (Q2) (A6)
- [ ] GUI-PATH-002: a set or invoke on a path absent from the movie returns a typed absent-path result and a diagnostic, never a silent no-op (A6)
- [ ] GUI-PATH-003: a path operation issued before its target exists is queued until the target appears or refused with the absent-path result, chosen per call and stated in the call
- [ ] GUI-PATH-004: injected adapter members use the `__byro` prefix, and every one whose absence is functional is listed by exact name in `RESERVED_ENGINE_CALLBACKS` (`crates/ui/src/host.rs`)
- [ ] GUI-PATH-005: the path service is the only engine code that reads, writes or invokes a movie member by path; any extender `UI` function `LMC` maps reaches a movie through it and no second route; scripts gain no path access
- [ ] GUI-HUD-001: with `--hud` on Skyrim the vanilla `hudmenu.swf` health, magicka and stamina meters follow the player's composed actor values (A4)
- [ ] GUI-HUD-002: the Fallout 4 health and action-point meters do the same (A5)
- [ ] GUI-HUD-003: the meters are fed through the paths Phase 0's verify listed, the ones the original engine drives
- [ ] GUI-HUD-004: when an original HUD drives the player's vitals, the native HUD's vitals bars do not draw a second copy
- [ ] GUI-HUD-005: the vanilla Skyrim and Fallout 4 compasses follow the camera heading
- [ ] A4: new `m48-skyrim-hud-meters.sh` (same fixture as `m48-6`), indexed in `docs/smoke-tests/README.md`: between captures at `hud.values 1.0 1.0 1.0` and `hud.values 0.35 1.0 1.0` the health band's changed-pixel count is above zero and a control band's is zero (GUI-HUD-001)
- [ ] A5: new `m48-fo4-hud-meters.sh` on MedTekResearch01, the same shape for health and AP (GUI-HUD-002)
- [ ] A6: `avm1_adapter_sets_and_reads_a_root_path` and `avm2_adapter_reports_an_absent_path` pass in `cargo test -p byroredux-ui`, on synthetic movies built the way the existing Ruffle-fixture tests build theirs (GUI-PATH-001/002)
- [ ] HUD meter liveness published in the ROADMAP M48 row for Skyrim and Fallout 4 (§8)
- [ ] A1: the regression floor passes unchanged

**Exit:** scenarios 4, 5 and 6 pass; HUD meter liveness is published for Skyrim and Fallout 4.

