# UI-D6-2026-09-21-01: --menu plus a MenuXml --hud leaves a focused UiManager that is never ticked, rendered or drained yet captures all window input

**Issue**: #4723
**Severity**: LOW
**Labels**: low,ui,bug

## Description
`scaleform_hud::launch` refuses to install a second Scaleform driver when `ui_manager` already holds a `--menu`-launched player ("would orphan its host-call draining"), but `hud::launch_hud` (the MenuXml HUD path) has no such guard — its signature doesn't even take `ui_manager` as a parameter. With `--menu … --menu-archive … --hud` on an Oblivion/FO3/FNV `--esm`, both get constructed: the `--menu` player keeps `visible = true` and the input focus `install_player` granted, but the frame loop's per-frame branch prefers `hud` over `ui_manager` whenever both exist (`scene.rs`'s own comment documents this preference), so the `--menu` player is never ticked, rendered, or drained again, and its overlay texture handle is overwritten by the HUD's.

`route_scaleform_window_event` still captures every window event for the now-frozen `--menu` player (because it still holds input focus), and `about_to_wait`/`device_event` release world input every frame regardless, so movement, mouse look, and Escape → pause all go dead — only F3 and the debug server keep working. `extension_ui_menu_sync` also keeps publishing the frozen, invisible menu as "visible".

## Evidence
Verified at HEAD `ee6d3fb39`:
- `crates/ui/src/lib.rs` / `byroredux/src/scaleform_hud.rs::launch`: `if ui_manager.is_some() { ... return None }` (or equivalent early refusal) is present.
- `byroredux/src/hud.rs::launch_hud(ctx, world, args)` — signature has no `ui_manager` parameter, so it cannot check or clear the existing manager's state.
- `byroredux/src/scene.rs:254-269`: the call sequence — `launch_archive_menu` (populates `ui_manager` from `--menu`) then `scaleform_hud::launch` (guarded) then, only if that returned `None`, `hud::launch_hud` (unguarded) — with the comment explicitly naming the "`--menu` + a MenuXml game" combination as the one that can produce both.
- `byroredux/src/app_frame.rs:391-420`: the frame-tick branch prefers the `hud` field over `ui_manager` when both are populated.

## Impact
Developer-only flag combination (`--menu` combined with `--hud` on a MenuXml-track game); not reachable from any documented single-flag launch. When triggered, world movement, look, and pause are dead for the rest of the session, with no error logged. No data-integrity risk.

## Related
- UI-D7-2026-09-21-05 (this report) — a related per-driver-inconsistency finding.
- The pending menu-stack/focus policy (report §5) — this is one of the concrete cases that policy needs to own.

## Suggested Fix
Apply the same guard `scaleform_hud::launch` uses to `launch_hud`: pass `ui_manager` (or a `bool` "already has a live UI player") in and skip-and-log when it's occupied. Alternatively, when `--hud` wins the race, explicitly clear the `--menu` player's input focus and either keep ticking it (composited but unfocused) or tear it down. Pin whichever behavior is chosen with a small integration test.

## Completeness Checks
- [ ] **TESTS**: A test covers `--menu` + `--hud` on a MenuXml game and asserts input focus lands somewhere that still routes to world/pause, not to a frozen player

Source: docs/audits/AUDIT_UI_2026-09-21.md (UI-D6-2026-09-21-01)
