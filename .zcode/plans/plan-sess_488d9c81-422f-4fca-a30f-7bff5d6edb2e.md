# M48.7 — Fallout 4 HUD: the `--hud` Scaleform route on AVM2

**Premise (verified):** FO4's HUD is `interface\hudmenu.swf` in `Fallout4 - Interface.ba2` (present on disk; the `--menu` route already loads it green with `profile=Some(Fallout4Avm2) state=Some(AdapterInjected)`). The M48.6 Skyrim driver's skeleton is reusable nearly wholesale — the host bridge is profile-blind (same drain, `ScaleformValue` marshalling, `available_callbacks`/`invoke_callback`/`set_response_handler` all work for AVM2; BGSCodeObj calls arrive as `BGSCodeObj.<Method>`-namespaced ExternalInterface calls via the injected ABC adapter). FO4 Health/AP actor values are `0x2C9`/`0x2D0` — already stamped on FO4 NPCs by `derive_stored_actor_values`, and already sitting in `hud.rs`'s `FALLOUT_BARS`. Same negative finding as Skyrim: no meter-shaped BGSCodeObj methods exist, so meters stay engine-empty (GFx object-path limitation, now documented for both games) — FO4 gets the same chrome + protocol + console slice.

## Step 0 — discovery: FO4 hudmenu's live surface

One `--menu` run (existing route) with host-call diagnostics + screenshot: the BGSCodeObj calls vanilla FO4 hudmenu makes at boot, and what its chrome draws statically (calibration for the smoke diff gate). The adapter guarantees `__byroBGSCodeObjReady` is registered when injected — a runtime AdapterInjected observable the smoke can gate on.

## Step 1 — generalize `scaleform_hud.rs` to two games

- `ScaleformGame` (enum or small profile struct): `Skyrim` (unchanged: `Skyrim - Interface.bsa`, Skyrim AV keys 0x3E8/9/A, 3 bars) and `Fallout4` (`Fallout4 - Interface.ba2`, `interface\hudmenu.swf`, hp `0x2C9` / ap `0x2D0`, labels health/ap, 2 bars). `hud_scaleform_args` probes both archives beside `--esm` and returns the game + archive path.
- `hud.rs` skip-guard: extend the quiet `Ok(None)` arm (and the Err candidate text) with `Fallout4 - Interface.ba2` so a FO4 `--hud` launch reports one failure, not two.
- `hud: loaded` line gains `profile=`/`state=` (from `UiManager::menu_profile()` + `host_object_state()`) — makes `state=Some(AdapterInjected)` smoke-greppable on the `--hud` route for both games.
- Response handlers: gate the SkyUI poll handlers (`updateStats`/`RequestPlayerInfo`) on the Skyrim profile — on FO4 they'd be catalog-unknown noise. No fabricated FO4 handlers; `unknown/unanswered` via `hud.debug` stays the discovery instrument.
- Push table: skip the adapter's own `__byro*`-prefixed callbacks; otherwise unchanged (name-matched, discovery-driven).

## Step 2 — tests

- `crates/ui/tests/fallout4_hudmenu_protocol.rs` (mirror of the Skyrim pin, env-gated self-skipping on `BYROREDUX_FO4_DATA`): extract `interface\hudmenu.swf` → `host_object_state() == AdapterInjected`, lifecycle callbacks present, movie host-call surface ⊆ FO4 catalog, resource errors empty.
- Driver launch dispatch (Skyrim vs FO4 selection) is covered by the smokes; no new hud.rs unit tests (consistent with M48.4–.6).

## Step 3 — smoke `m48-7-fo4-hud.sh` + README row

FO4 fixture args (MedTekResearch01, Meshes/MeshesExtra/Textures1/Materials) + `--hud`, port-scoped like m48-6. Gates: `hud: loaded … backend=scaleform … state=Some(AdapterInjected)`; `hud.status` shows the 2 FO4 bar pins (`health=`/`ap=`) + `backend=scaleform`; `hud.debug` shows `__byroBGSCodeObjReady` among callbacks + driver liveness; chrome on/off pixel diff (same world-static technique as m48-6, bands calibrated in Step 0 — if vanilla FO4 hudmenu draws no static chrome, the gate degrades to protocol-only and that gets documented, not fudged). README row, SKIP-77 without data.

## Step 4 — docs + verification

- ROADMAP M48.7 paragraph; `docs/engine/ui.md` M48.6 section extended with the FO4 dispatch (profile/state on the log line, adapter lifecycle notes: destruction ack conditional per `AdapterInjectedWithoutDestroyHook`); AGENTS.md tree line update + smoke list; smoke README row.
- Verification: `cargo test -p byroredux-ui` (pins + new protocol test, `-j` capped), menuxml untouched-green, bin check via 1.96.0 toolchain, smokes m48-4/m48-5/m48-6/m48-7 + m48-menu-load skyrim leg (fo4 leg too, now that the #4466 cargo fix landed).

**Non-goals:** driven meters (same Ruffle object-path limitation as Skyrim, already documented); power-armor HUD (`powerarmorhudmenu.swf`); Pip-Boy; font fidelity; menu stack.

**Risks:** Ruffle AVM2 runtime completeness on hudmenu (mitigated: the m48-menu-load FO4 leg is green and the installed lifecycle test runs it headlessly); FO4 hudmenu may draw no static chrome until pushed (Step 0 decides the pixel-gate shape honestly); FO4's modded install may add menu content (fixture BSAs are the vanilla set; Interface.ba2 confirmed vanilla-named).