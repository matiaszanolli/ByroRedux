# #6653: M48 Phase 1: menu registry and stack, pause and menu mode from the registry

Part of #6641. PRD [§9 Phase 1](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/game-ui-integration.md#9-delivery-sequence). Depends on Phase 0 (#6643: the design section, the per-game inventory and flags table, the `Begin MenuMode` block number) and on M47.2 / M47.3 for the script queries and blocks it serves (`MenuMode`, `Begin MenuMode`, `Utility.IsInMenuMode`). The [VATS PRD](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/vats.md)'s menu phase depends on this phase. A*N* = §7.2 scenario *N*.

Open bugs in the code this phase replaces: #6620 (`menu_open` answers the FO3 / FNV codes on Oblivion) and #6576 (`publish_ui_menus` reports Pause for the crafting page).

## Acceptance Criteria
- [ ] GUI-REG-001: one menu registry lists every open menu by its game-facing name, in stacking order, with its surface and flags (A2)
- [ ] GUI-REG-002: the extender `UI.IsMenuOpen` (SDK alias `ui.menu-is-open`), ObScript `MenuMode` and every engine-side "is a menu open" check read the registry through per-game name and code tables in the translate step; `MenuMode` 0–3 count only menu-mode menus, so an open HUD alone answers `false` (A2)
- [ ] GUI-REG-003: a HUD menu stays in the registry, reported open, while a modal is open above it; whether it stays drawn follows the modal's flags (A2, A3)
- [ ] GUI-REG-004: each menu's flags and menu-mode membership come from the Phase 0 per-game table, every entry cited or recorded as an engine default; the VATS menu entry carries no pause (`VATS-UI-007`)
- [ ] GUI-REG-005: open movies share one wgpu device instead of one per `SwfPlayer`; `ui.menus` reports the device count (A3)
- [ ] GUI-REG-006: the registry's game-facing names, codes and target surfaces come from the checked-in Phase 0 inventory
- [ ] GUI-REG-007: closing, replacing or failing to load a menu returns input to the next owner in the stack (or the world) in the same frame (A3)
- [ ] GUI-REG-008: after a swapchain resize every open movie and document re-renders at the new extent instead of stretching, per the Phase 0 design
- [ ] GUI-REG-009: at most one registry menu owns input, the topmost whose flags claim input; `ui.menus` names it (A3)
- [ ] GUI-REG-010: the simulation advances or pauses by the PauseGame flag of the open registry menus, whatever their surface; `ui.menus` reports the pause state; the VATS menu pauses nothing on its own account (A10)
- [ ] GUI-REG-011: Papyrus `Utility.IsInMenuMode` answers `true` exactly while the registry holds a menu whose flags pause the game, so an open dialogue menu answers `false` (A2)
- [ ] GUI-PAGE-001: every native page registers under the game-facing name of the menu it stands in for (`InventoryMenu`, `Dialog`, code `1009`, …) (A2)
- [ ] GUI-EVT-005: a menu open changes no saveable state; the registry is presentation state republished every frame (`NOT_SAVED_BY_DESIGN`)
- [ ] GUI-EVT-006: ObScript `Begin MenuMode [code]` blocks run on the frames the registry holds a matching menu-mode menu (any one when the block names no code), and `Begin GameMode` blocks do not run on those frames, even while the simulation is paused (A10)
- [ ] GUI-OPS-001: `ui.menus` lists every registry entry with name, surface, flags and input owner, plus the open-movie device count and pause state; `hud.status` / `hud.debug` stay (A3)
- [ ] GUI-OPS-005: `ui.open <menu>` / `ui.close <menu> [accept|cancel]` open a menu by its game-facing name through the registry and close it with the stated result; the next `ui.menus` line shows the change (A3)
- [ ] A2: `menu_registry_answers_is_menu_open_and_menu_mode_alike` passes in `cargo test -p byroredux --bin byroredux` (GUI-REG-001/002/003/011)
- [ ] A3: new `m48-menu-stack.sh` (Skyrim SE, BleakFallsBarrow01, `--hud`) passes, indexed in `docs/smoke-tests/README.md`: each `ui.menus` line through `input.press inventory`, `ui.open MessageBoxMenu`, `ui.close MessageBoxMenu cancel` shows the HUD open, the expected input owner and a device count of 1 (GUI-REG-003/005/007/009, GUI-OPS-001/005)
- [ ] A10: `menu_mode_blocks_and_pause_follow_the_registry` passes in `cargo test -p byroredux --bin byroredux` (GUI-REG-010, GUI-EVT-006)
- [ ] A1: the regression floor passes unchanged

**Exit:** scenarios 2, 3 and 10 pass, and scenario 1's floor passes.

