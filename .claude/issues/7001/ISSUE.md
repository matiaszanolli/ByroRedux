# #7001: UI-D7-2026-11-15-01: The Oblivion sneak launch names the wrong tile as "crosshair", so the grafted reticle's own crosshair still draws on top of the native one

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/7001
- **Labels**: low,ui,legacy-compat,game:oblivion,bug
- **Source**: docs/audits/AUDIT_UI_2026-11-15.md

From the UI audit `docs/audits/AUDIT_UI_2026-11-15.md` (HEAD 8473ae994).

- **Severity**: LOW
- **Dimension**: MenuXml & HUD Drivers
- **Profile**: MenuXml (Oblivion `--hud`)
- **Location**: `byroredux/src/hud.rs:171` (`crosshair: "hudreticle_reticle"`), `hud.rs:828` (`renderer.set_override(crosshair, "visible", 0.0)`), doc at `hud.rs:114-118`
- **Status**: NEW (014e64ef1)
- **Description**: `install_sneak_element` grafts `hud_reticle.xml` under `HUDMainMenu` to get `hudreticle_sneak`, and is meant to hide the reticle's crosshair "(the native HUD draws one)". It hides the tile named `hudreticle_reticle`, which is the container `<rect>`. The image that actually draws the crosshair is its child `hudreticle_reticle_crosshair` (`menus\main\hud_reticle.xml`: `<image name="hudreticle_reticle_crosshair">`, 37 x 37, `visible = (parent.user5 == 0)`, `alpha` copied from `hudreticle_reticle_activate`, i.e. the parent's `user2` = 128). `visible` is per-tile in this dialect (`layout.rs:148` reads each tile's own `visible`, and `walk_children` descends regardless of the parent's, `layout.rs:244`; the vanilla XML copies visibility into children explicitly), so hiding the container hides nothing.
- **Evidence** (probe in the scratch worktree, Oblivion data, 1280x720, HUD in explore mode, same overrides the launch applies): rendering with the graft and the launch's overrides versus no graft differs in 438 pixels inside the box (622,342)-(658,378), the screen centre. Hiding `hudreticle_reticle_crosshair` as well makes the difference 0 pixels; hiding `hudreticle_reticle_activate`, `hudreticle_enemy_health` or `hudreticle_sneak` instead changes nothing (still 438). `the_oblivion_hud_lights_the_sneak_eye` cannot see this: it diffs the "no indicator" frame against the "lit" frame, both of which carry the stray crosshair (1570 differing pixels, passes).
- **Impact**: with `--hud` on Oblivion, a vanilla reticle crosshair (alpha 128) is composited over the screen centre in addition to the native egui crosshair (`panels/hud.rs:9-23`), for the whole session including when the eye is off. Cosmetic; `--hud` is the vanilla-HUD front door.
- **Related**: #6913 / 014e64ef1; Dim 5 (the overlay itself is fine).
- **Suggested Fix**: put the child's name in the profile (`crosshair: "hudreticle_reticle_crosshair"`), and add a pixel assertion to the Oblivion sneak test that the no-indicator frame equals the no-graft frame.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or an extended guard) pins this specific fix
