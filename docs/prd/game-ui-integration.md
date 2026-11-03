# Game UI Integration — Product Requirements

**Status**: PROPOSED (2026-11-03). Revision 3 folds in review round 2:
§3.1 re-verified at HEAD (no close path, one native page at a time, the
native pause freezing every script, ObScript menu requests, `MenuMode`
blocks and `Utility.IsInMenuMode` unserved), the pause flag and the
vanilla menu-mode queries added as requirements, the FO3 / FNV menu codes
sourced from xEdit, the path service held to the SDK plan's capability
model, and the acceptance scripts renamed off the Starfield `m48-8` slot.
Revision 2 folded in review round 1; revision 1 was the first draft of the
M48 remainder: what the shipped Scaleform and MenuXml slices leave open,
and the decisions it needs from the project owner (§10). Statements about
how an original engine, file format or tool behaves cite external sources
(the Creation Kit / GECK / CS wikis, xEdit's record definitions, Autodesk's
Scaleform documentation, the Ruffle project); a claim that could not be
verified is marked **Phase 0: verify** instead of being stated as fact.

**Scope**: The player-facing user interface of the six single-player
target titles: which surface renders each of a game's menus (the game's own
Scaleform movie, its own MenuXml document, or an engine-native page), how
open menus stack and own input, what the engine does when a menu calls the
host, how the engine and scripts read and drive menu state, fonts and
text, and the residues other milestones handed to M48 (vanilla Skyrim /
Fallout 4 HUD meters, the race menu, the dialogue page's presentation).
Out of scope: UI *mod* compatibility as a promise (owned by the
[Legacy Mod Compatibility PRD](legacy-mod-compatibility.md#56-ui-lmc-ui),
`LMC-UI-*`, which this document serves), the Mod Configuration Menu host
(`LMC-UI-002`), gameplay minigames (lockpicking, hacking), what the VATS
menu shows and which VATS functions its controls call (the
[VATS and Action Points PRD](vats.md), whose menu runs on this document's
registry `GUI-REG` and MenuXml menus `GUI-XML-001`…`003`), and Fallout 76.

**Related**: this document states the *promise*; the designs below own the
*how*. Every mechanism promised here has an owner in this table, or Phase 0
writes the missing design.

| Mechanism | Owning design |
|---|---|
| Ruffle host bridge, host profiles, catalogs, AVM2 adapter, navigator, input routing | [UI System](../engine/ui.md) |
| Scaleform HUD route (`--hud`, Skyrim / Fallout 4) | [UI System § Scaleform HUD](../engine/ui.md#scaleform-hud-m486-skyrim--m487-fallout-4) |
| MenuXml track (Oblivion / FO3 / FNV) | [UI System § MenuXml legacy UI](../engine/ui.md#menuxml-legacy-ui-m484-oblivion--m485-fo3) |
| Menu registry, menu stack, path-addressed access | none today — Phase 0 writes it as a section of [ui.md](../engine/ui.md#input-routing) |
| Loading screens | [UI System § loading screens](../engine/ui.md#original-game-loading-screens-during-scene-transitions) |
| Native dialogue page and the dialogue layers behind it | [Dialogue Trees](../engine/dialogue-trees.md#3-scope) |
| VATS menu content and controls (a consumer of `GUI-REG` and `GUI-XML-001`…`003`) | [VATS and Action Points PRD](vats.md) |
| Bethesda menu catalog and the Papyrus UI surface | [Creation Engine UI](../legacy/creation-engine-ui.md) |
| UI mod compatibility (`.swf` / MenuXml replacements, MCM, HUD frameworks) | [Legacy Mod Compatibility PRD § UI](legacy-mod-compatibility.md#56-ui-lmc-ui) |
| Extender UI functions and the semantic UI contribution service | [SDK plan §7](../engine/sdk-v0.1-development-plan.md#7-script-extender-replacement-map) |

Normative terms **MUST**, **SHOULD** and **MAY** have their RFC 2119
meanings, as in the related designs. Requirement IDs use the `GUI-` prefix.

---

## 1. Product thesis

The SWF/GFx strategy is decided and is not reopened here: R4 chose pinned
Ruffle as the Flash VM with ByroRedux-owned Scaleform host profiles,
*preserving SWF mod compatibility*
([R4, roadmap-history](../archive/roadmap-history.md#tier-7--deep-gameplay-systems-deferred-until-tier-14-proves-out)).
The pre-Skyrim games keep their own XML UI on a separate track (M48.4 /
M48.5). Both tracks now load the vanilla HUD over the world. What they do
not yet do is *behave*: menus call the host and get nothing back, the
Skyrim and Fallout 4 meters stay empty, one menu can be open at a time,
scripts that open or watch menus see either nothing or a different answer
depending on which script language asked (and no script runs at all while
a native page other than dialogue is open), and every player-facing page
except the HUD is an engine-native page drawn by the engine's own overlay.

This PRD is for the player of each target title, and through
`LMC-UI-001` / `LMC-UI-003` for every UI mod author whose movie or XML must
run inside the same host. The promise is that a game's menus are *that
game's menus*, driven by canonical engine state, wherever the engine can
serve the menu's host contract — and that where it cannot, the engine says
so per menu instead of failing silently.

The corollary that shapes every requirement below: **a menu is either
served or reported, never silently half-alive.** Every menu of every game
has exactly one surface (§3), every host method a movie calls has exactly
one behaviour class, and both are visible from `byro-dbg` without a
debugger. The engine will not clone the Scaleform GFx runtime, will not
fork Ruffle's VM, and will not fabricate host answers it has no canonical
state for.

---

## 2. Terminology

| Term | Meaning |
|---|---|
| **Menu** | One named player-facing screen of a game, under the name the game's own scripts use (`InventoryMenu`, `HUD Menu`, `Dialog`, menu code `1009`). The unit of this document's promise. |
| **Surface** | What renders a menu: one of the four classes in §3. |
| **Original movie** | A game's own `.swf` from its interface archive (or a mod's replacement), run in Ruffle under a host profile ([ui.md](../engine/ui.md#why-ruffle)). |
| **Original document** | A game's own MenuXml `menus\*.xml`, run by `byroredux-menuxml`. |
| **Native page** | An engine-drawn page of the player menu (`GameMenuPage` in `crates/debug-ui/src/panels.rs`: Pause, Settings, Inventory, Dialogue, MessageBox, Barter, Crafting) or the native HUD layer (`draw_hud`, same file). |
| **Host method** | A call a movie makes into the engine: a Skyrim `GameDelegate` method or a Fallout 4 `BGSCodeObj` function, recognised through the profile's catalog ([ui.md](../engine/ui.md#scaleform-host-methods-and-remaining-_globalgfx-stubs)). |
| **Behaviour class** | The engine's answer to one host method: one of H1–H4 (§3.2). |
| **Path-addressed access** | Reading, writing or invoking an ActionScript member by its dot path from the host (`_root.HUDMovieBaseInstance…`), the operation Scaleform exposes to C++ as `GFx::Movie::GetVariable` / `SetVariable` / `Invoke` ([Autodesk: Game Communication Overview](https://help.autodesk.com/cloudhelp/ENU/Scaleform-Help/scaleform_help/game_communication.html); [Autodesk: ActionScript](https://help.autodesk.com/cloudhelp/ENU/Scaleform-Help/scaleform_help/game_communication/c_actionscript.html)). |
| **Adapter injection** | Inserting a ByroRedux-owned bytecode adapter into a movie at load so the host can reach movie state through Ruffle's supported ExternalInterface boundary. Shipped for AVM2 (`crates/ui/src/avm2_host.rs`); the AVM1 counterpart is the "AVM1 injection" the ROADMAP M48 row names. |
| **Menu registry** | The one engine record of which menus are open, in which order, with which flags. Does not exist at HEAD (§3.1 row 2); `GUI-REG` defines it. |
| **Menu flags** | Per-menu behaviour bits (pauses the game, shows the cursor, allows saving, blurs the world) plus a depth. The only cited bit definitions are F4SE's `MenuData` struct for *custom* Fallout 4 menus (`MenuFlags` 0x01 PauseGame, 0x04 ShowCursor, 0x800 DoNotPreventGameSave, 0x400000 BlurBackground; `Depth` a separate field), which gives one vanilla menu's value (MessageBoxMenu 0x8018499) and notes that not all flags are known ([FO4 CK: MenuData Struct](https://falloutck.uesp.net/wiki/MenuData_Struct_-_UI)). |
| **Menu mode** | The state scripts observe while a menu is open: ObScript's `Begin MenuMode` / `Begin GameMode` blocks and `MenuMode` function ([GECK: MenuMode](https://geck.uesp.net/wiki/MenuMode), [GECK: GameMode](https://geck.uesp.net/wiki/GameMode), [CS: MenuMode](https://cs.uesp.net/wiki/MenuMode)), and Papyrus `Utility.IsInMenuMode`, which the CK defines by the game being paused, so that dialogue does not count ([CK: IsInMenuMode](https://ck.uesp.net/wiki/IsInMenuMode_-_Utility), [FO4 CK: IsInMenuMode](https://falloutck.uesp.net/wiki/IsInMenuMode_-_Utility)). |
| **Menu code** | The number ObScript names a menu by (`MenuMode 1002`). Per game: [CS: MenuTypes](https://cs.uesp.net/wiki/Template:MenuTypes) for Oblivion, [GECK: MenuTypes](https://geck.uesp.net/wiki/Template:MenuTypes) for FO3 / FNV, and xEdit's `wbMenuModeEnum` (`wbDefinitionsCommon.pas`), which lists Oblivion's and FO3 / FNV's codes in one table. |

---

## 3. Core model

### 3.0 Surfaces

Every menu of every game has exactly one surface at any time:

| Class | Name | Meaning | Reported as |
|---|---|---|---|
| **S1** | Original movie | The game's (or a mod's) `.swf` under the game's host profile | `surface=movie <path> profile=<profile>` |
| **S2** | Original document | The game's (or a mod's) MenuXml document under its `MenuProfile` | `surface=menuxml <path>` |
| **S3** | Native page | An engine-native page stands in for the menu | `surface=native <page>` |
| **S4** | Absent | No surface; a script that opens the menu gets the documented fallback: a placeholder effect, a recorded request (today's `script.requests` log) or a decline | `surface=absent fallback=<placeholder\|recorded\|declined>` |

Which surface a menu *should* have is decision Q1. This document's
requirements hold for every surface: one registry, one input owner, one
behaviour class per host call, one report.

### 3.1 State at HEAD

The inventory below is the state at HEAD on 2026-11-03 (re-verified for
revision 3; first written 2026-11-01). It is hand-written and will go
stale; Phase 0 replaces it with the per-game menu inventory (GUI-REG-006),
after which this table is generated from that inventory or removed.

| # | Item | Today |
|---|---|---|
| 1 | Shipped slices | Host bridge, Skyrim/SkyUI and Fallout 4 `BGSCodeObj` catalogs, generated AVM2 adapter, archive navigator, MenuXml HUDs for Oblivion / FO3 / FNV, `--hud` over vanilla Skyrim and Fallout 4 `hudmenu.swf`: the [ROADMAP M48 row](../../ROADMAP.md#tier-7--deep-gameplay-systems) is the inventory and owns the catalog sizes |
| 2 | Open movies | One: `UiManager` holds `player: Option<SwfPlayer>` and one `menu_name` (`crates/ui/src/lib.rs:57-65`). Each `SwfPlayer` creates its own wgpu instance and device (`create_wgpu_instance` / `request_adapter_and_device`, `crates/ui/src/player.rs:83-88` and `:125-130`). There is no close: a movie is replaced (`install_player`, `lib.rs:151`) or hidden (`hud.off`), never unloaded; #2723 deleted the caller-less `UiManager::close` (`lib.rs:356-364`) |
| 3 | Native pages | `GameMenuPage` = Pause, Settings, Inventory, Dialogue, MessageBox, Barter, Crafting, drawn with egui in `crates/debug-ui/src/panels.rs:468`; one page at a time (`GameMenuState.page`, `panels.rs:481-487`). The native HUD layer (crosshair, prompts, vitals, objectives) is `draw_hud` (`panels.rs:246`); its vitals draw whenever the `interface.show_vitals` setting (default on, `byroredux/src/app_frame.rs:301`) is set and the player has vitals (`panels.rs:264`), whether or not an original HUD is up |
| 4 | Host calls | Drained once per frame and logged; an `Unknown` or `MissingResponse` call warns once per (menu, method); no call is routed into game state (`drain_host_calls` loop, `byroredux/src/app_frame.rs:1401-1483`) |
| 5 | Host answers | The only production response handlers are the Scaleform HUD driver's (`updateStats`, `RequestPlayerInfo`, Skyrim-profile only; `set_response_handler` at `byroredux/src/scaleform_hud.rs:298` is the only production caller, and no static `set_response` value is registered anywhere). The bridge itself consumes the adapter's `__byroBGSCodeObjDestroyed` acknowledgement (`record_call`, `crates/ui/src/host.rs:443-449`); the other `__byro*` hooks are engine-to-movie callbacks, not answers |
| 6 | Vanilla HUD meters and compass | Empty on Skyrim and Fallout 4: the movies make no stats call (`hudmenu_host_calls_are_pinned_and_cataloged`, `fallout4_hudmenu_lifecycle_and_calls_are_pinned` in `crates/ui/tests/`), and the HUD driver's push table (`updateStats`, `updateCompass` and their capitalised forms) finds no registered target in a vanilla movie (`ScaleformHudDriver::push` doc, `byroredux/src/scaleform_hud.rs:430-444`) |
| 7 | Scaleform extensions | No `_global.gfxExtensions` / `scaleform.gfx.*` surface exists (`rg _global crates/ui/src`, `rg gfxExtensions crates/ui/src` and `rg scaleform.gfx crates/ui/src` each match nothing; the only `gfx` hits are the AVM1 scanner's `gfx.io.GameDelegate` comments in `crates/ui/src/avm1_host.rs` and its tests) |
| 8 | Fonts | `fonts_en.swf` reaches Fallout 4 through `ImportAssets` ([ui.md](../engine/ui.md#font-loading)); no `FontConfig.txt` is read (`rg -i fontconfig crates/ui byroredux/src` finds nothing) |
| 9 | Extender `UI.IsMenuOpen` | SKSE / F4SE's function ([CK: UI Script](https://ck.uesp.net/wiki/UI_Script), [FO4 CK: UI Script](https://falloutck.uesp.net/wiki/UI_Script)), served through the SDK alias `ui.menu-is-open` (`crates/sdk/src/compatibility/source_alias.rs:227`). It answers from the one `UiManager` movie name (`extension_ui_menu_sync`, called from `byroredux/src/app_events.rs:1472`); a native page is never "open" to it |
| 10 | ObScript `MenuMode` | Answers from `UiMenuState`, built from the native pages, the operator overlay (code 3), the loading cover (1007) and the message-box and barter queues (codes 0–3, 1001, 1002, 1007, 1009, 1013, 1053; `menu_open`, `byroredux/src/systems/script_queries.rs:55-72`), with no per-game code table; a Scaleform movie is never "open" to it |
| 11 | Menu events to scripts | No handler for Fallout 4's `RegisterForMenuOpenCloseEvent` / `OnMenuOpenCloseEvent`: the only match in the code is the `.pex` decompiler's event-name table (`crates/pex/src/decompile/event_names.rs:114`), which names the event and runs nothing; SKSE's `RegisterForMenu` is classified unsupported, "await the isolated UI contribution service" (`classify_method_call`, `crates/sdk/src/compatibility/source_alias.rs:489-492`) |
| 12 | Race menu | `Game.ShowRaceMenu()` lowers to `Effect::ShowRaceMenu`, a placeholder that auto-accepts the current appearance (`crates/scripting/src/translate/effects/model.rs:513-518`, `is_placeholder`); only the zero-argument form lowers (`crates/scripting/src/translate/effects/prims/player_control.rs:110-111`); ObScript `ShowRaceMenu` does the same (`crates/scripting/src/obscript_more/tail.rs:252-258`). ROADMAP lists it as the MQ101 residue in [Known Issues](../../ROADMAP.md#known-issues) |
| 13 | Script-opened menus | `Actor.ShowBarterMenu()` opens the native Barter page (`crates/scripting/src/menus.rs`, `byroredux/src/barter.rs`). ObScript's menu commands (`ShowMap`, the character-creation screens including `ShowRaceMenu`, `ShowRecipeMenu`, the casino games, …) queue a `MenuRequest` under an engine-coined name (`"special book"`, `"vigor tester"`, …; `crates/scripting/src/obscript_more.rs:91-102`) that `script_ui_request_system` logs and drops (`byroredux/src/systems/script_requests.rs:403`); `script.requests` lists them (`byroredux/src/commands/quest.rs:1550`) |
| 14 | Resize | The UI viewport is fixed at `UiManager::new`; a swapchain resize stretches the overlay (field doc, `crates/ui/src/lib.rs:68-74`). #2723 was closed by documenting this, not fixing it (`7390fe4c`): Ruffle's `TextureTarget` / `WgpuRenderBackend` have no in-place resize |
| 15 | MenuXml beyond the HUD | `HudGameProfile` drives `menus\main\hud_main_menu.xml` only (`byroredux/src/hud.rs:139`, `:172`); FO3/FNV `-s`-prefixed strings are not resolved (`MenuProfile` doc, `crates/menuxml/src/profile.rs:36-40`) |
| 16 | Pause | Every native page except Dialogue freezes the whole scheduler (`DebugUiState::simulation_paused`, `crates/debug-ui/src/lib.rs:348-350`; `scheduler.run` skipped in `byroredux/src/app_events.rs:1546-1580`), so no script runs while one is open. A Scaleform movie never pauses anything |
| 17 | Menu-mode script surfaces | ObScript runs `Begin GameMode` and the event blocks only: object scripts have no `MenuMode` arm (`crates/scripting/src/obscript_objects.rs:367`) and quest scripts run their GameMode block only (`run_quest_game_mode`, `crates/scripting/src/obscript_quests.rs:342`), so a `Begin MenuMode` block never runs. Papyrus `Utility.IsInMenuMode` has no handler (`rg -i isinmenumode` matches no Rust source) |

### 3.2 Host-method behaviour classes

The repository already distinguishes *recognised* from *served*: catalog
entries "are recognition and protocol metadata, not claims that gameplay
behavior exists" ([ui.md](../engine/ui.md#scaleform-host-methods-and-remaining-_globalgfx-stubs)),
and the scripting layer counts placeholder effects apart from real ones
(`Effect::is_placeholder`). This document applies both rules to host
methods:

| Class | Name | Meaning | Reported as |
|---|---|---|---|
| **H1** | Served | The engine acts on canonical state or answers from it | `served` |
| **H2** | Placeholder | A deliberate fixed answer with a cited reason (an online service, a platform feature, a modelled-elsewhere effect) | `placeholder: <reason>` |
| **H3** | Unserved | Recognised by the catalog, no handler; the movie receives `Null` | `unserved` (today's `Queued` / `MissingResponse`) |
| **H4** | Unknown | Not in the profile's catalog | `unknown` (today's `Unknown`) |

---

## 4. Goals and non-goals

### 4.1 Goals

- **GUI-G-001 — One registry.** Every menu of every in-scope game is listed
  with its surface (S1–S4), and every script-side and engine-side question
  about open menus is answered from that one list.
- **GUI-G-002 — Live vanilla HUDs.** The vanilla Skyrim and Fallout 4 HUD
  meters move with the player's canonical actor values, measured by a
  pixel gate per game.
- **GUI-G-003 — Behaviour coverage.** Every host method the vanilla
  interface archive of a wave-1 game calls has a behaviour class, and the
  H1 share is published per game.
- **GUI-G-004 — Stacked menus.** A modal menu opens over a live HUD and a
  second modal over the first, with one input owner, on every in-scope
  game.
- **GUI-G-005 — No placeholder residues.** The race menu and the dialogue
  page each reach the surface Q1 / Q4 choose, and the MQ101 Known Issue
  closes.

### 4.2 Non-goals

- **GUI-NG-001.** Reopening R4: no second Flash VM, no Ruffle fork, no
  Scaleform GFx linkage ([R4](../archive/roadmap-history.md#tier-7--deep-gameplay-systems-deferred-until-tier-14-proves-out);
  proprietary middleware is parsed, never linked).
- **GUI-NG-002.** Mod-side UI promises: `.swf` / MenuXml replacements,
  translation files, MCM and HUD frameworks are `LMC-UI-001` … `LMC-UI-005`.
  This document builds what they run inside; it does not restate their
  dispositions.
- **GUI-NG-003.** Online and storefront menus (Bethesda.net, Creations,
  platform achievements; the Creation Club is "an in-game microtransactional
  store" for Skyrim Special Edition whose credits Fallout 4 shares,
  [UESP: Creation Club](https://en.uesp.net/wiki/Skyrim:Creation_Club)):
  anti-scope ("no Bethesda.net-style mod marketplace",
  [ROADMAP](../../ROADMAP.md#what-we-are-not-doing-anti-scope)). Their host
  methods are H2 with that reason (GUI-HOST-003).
- **GUI-NG-004.** Gameplay minigames behind menus (lockpicking, terminal
  hacking): `byroredux/src/lockpick.rs` documents both as engine policy with
  no minigame; their rules belong to the gameplay milestones. Their menus
  are reported S4 until a minigame exists.
- **GUI-NG-005.** Face and body sliders (morph-driven character creation):
  a separate milestone per Q4; this document ships only the race/sex
  selection the race menu needs to stop being a placeholder.
- **GUI-NG-006.** Gamepad input translation: the input layer does not read
  gamepads yet ([ROADMAP, playable slice](../../ROADMAP.md#playable-vertical-slice));
  menus consume `UiInputEvent` (`crates/ui/src/input.rs`) and inherit
  gamepad support when the input layer adds it.
- **GUI-NG-007.** Fallout 76 (§7.1).
- **GUI-NG-008.** Per-pixel parity with the original menus' look, the
  same anti-scope rule as the renderer's ([ROADMAP](../../ROADMAP.md#what-we-are-not-doing-anti-scope)).
  Fidelity here means the game's own movie or document runs, not that a
  native page imitates it.

---

## 5. Requirements by area

### 5.1 Menu registry and stack (`GUI-REG`)

- **GUI-REG-001.** The engine MUST hold one menu registry listing every
  open menu by its game-facing name, in stacking order, with its surface
  and flags. Today there is none: `UiManager` holds one movie and the native
  pages publish a separate `UiMenuState` (§3.1 rows 2, 9, 10).
- **GUI-REG-002.** The extender `UI.IsMenuOpen` (served through the SDK
  alias, §3.1 row 9; whether it is served is `LMC`'s disposition), ObScript
  `MenuMode` and every engine-side "is a menu open" check MUST read the
  registry, so that the same open menu answers `true` to all of them; the
  type-level questions (`MenuMode` 0–3) count only the menus that count
  toward menu mode (GUI-EVT-006), so an open HUD alone answers `false`.
  Today a native inventory page is invisible to `UI.IsMenuOpen` and a
  Scaleform movie is invisible to `MenuMode` (§3.1 rows 9, 10). The name
  and code tables are per game and live in the translate step (the menu
  codes of §2, [GECK: MenuTypes](https://geck.uesp.net/wiki/Template:MenuTypes),
  [CS: MenuTypes](https://cs.uesp.net/wiki/Template:MenuTypes) and xEdit's
  `wbMenuModeEnum`, which gives Oblivion's barter menu code 1008 and
  FO3 / FNV's code 1053; the Skyrim menu-name list,
  [CK: UI Script](https://ck.uesp.net/wiki/UI_Script); the Fallout 4 list,
  [FO4 CK: Menu](https://falloutck.uesp.net/wiki/Menu)).
- **GUI-REG-003.** A HUD menu MUST stay in the registry, reported open,
  while a modal menu is open above it; whether it also stays *drawn* follows
  the modal's flags (GUI-REG-004). **Phase 0: verify** what the original
  reports and draws in that state, settled by the SKSE64 `MenuManager` /
  `IMenu` definitions ([github.com/ianpatt/skse64](https://github.com/ianpatt/skse64))
  for Skyrim, the F4SE equivalents
  ([github.com/ianpatt/f4se](https://github.com/ianpatt/f4se)) for Fallout 4,
  and the GECK / CS `MenuMode` pages for code 1004 under an open
  inventory.
- **GUI-REG-004.** Each menu's flags, and whether it counts toward menu
  mode (GUI-EVT-006), MUST come from a per-game table with a cited source
  per entry. The only cited bit definitions today are F4SE's
  custom-menu `MenuData` (§2), which give a vanilla value for
  MessageBoxMenu alone and state that not all flags are known
  ([FO4 CK: MenuData Struct](https://falloutck.uesp.net/wiki/MenuData_Struct_-_UI)).
  **Phase 0: verify** every other vanilla menu's flags on every game,
  settled by reading the menu flag definitions and per-menu constructors
  in the SKSE64 and F4SE sources (links in GUI-REG-003) and the CS / GECK
  menu pages; a menu or game with no documented value is recorded as such
  in the table, with the engine default it gets and why. The VATS menu's
  entry (code 1056 on FO3 / FNV, `VATSMenu` on Fallout 4) records that it
  never pauses the world on its own account, citing `VATS-UI-007` in the
  [VATS PRD](vats.md), whatever pause bit a source gives it.
- **GUI-REG-005.** Open movies MUST share one wgpu device rather than
  creating one per `SwfPlayer` (§3.1 row 2), so that stacking N menus does
  not create N GPU devices; `ui.menus` (GUI-OPS-001) MUST report the device
  count.
- **GUI-REG-006.** Phase 0 MUST check in a per-game menu inventory: every
  menu name from the sources in GUI-REG-002 with its target surface (per
  Q1), its flags source and its script entry points. It is data, not code,
  and replaces §3.1. The GECK `MenuTypes` template elides codes (its list
  carries `...` gaps from 1004 to 1080); xEdit's `wbMenuModeEnum` names
  the FO3 / FNV codes it elides and leaves the others unnamed on Fallout,
  and its entries credited to mods (JIP LN, lStewieAI) are `LMC`'s, not
  vanilla. **Phase 0: verify** the one code the two sources name
  differently, FO3 / FNV code 1004 (GECK: `HUDMainMenu`; xEdit:
  `MainMenu`), settled by the xNVSE source's menu-type constants
  ([github.com/xNVSE/NVSE](https://github.com/xNVSE/NVSE)) and a census of
  `MenuMode 1004` call sites in the `Fallout3.esm` and `FalloutNV.esm`
  scripts, and record any code no source names as unknown. The VATS menu
  (code 1056) is listed like any other; what it shows is the
  [VATS PRD](vats.md)'s.
- **GUI-REG-007.** Closing, replacing or failing to load a menu MUST return
  input to the next owner in the stack (or the world) in the same frame.
  Today only replacement transfers focus (`install_player`,
  `crates/ui/src/lib.rs:151`) and nothing closes a movie (§3.1 row 2);
  the native pages release world input on open
  ([ui.md](../engine/ui.md#input-routing)).
- **GUI-REG-008.** The UI viewport MUST follow swapchain resizes: after a
  resize every open movie and document re-renders at the new extent
  instead of stretching. #2723 closed by documenting the stretch, because
  Ruffle's render target has no in-place resize (§3.1 row 14); the Phase 0
  design states how a movie's renderer is rebuilt instead.
- **GUI-REG-009.** At most one registry menu MUST own input at a time, and
  it MUST be the topmost menu whose flags claim input; `ui.menus`
  (GUI-OPS-001) names it. Today input focus is one `UiManager` flag plus
  the native pages' modal capture
  ([ui.md](../engine/ui.md#input-routing)).
- **GUI-REG-010.** Whether the simulation advances while menus are open
  MUST follow the PauseGame flag (GUI-REG-004) of the open registry menus,
  whatever their surface; `ui.menus` (GUI-OPS-001) MUST report the pause
  state. Today the rule is per surface: every native page except Dialogue
  freezes the whole scheduler and a Scaleform movie freezes nothing (§3.1
  row 16). The VATS menu is the stated exception: it never pauses the
  world on its own account, and world time during VATS belongs to the
  [VATS PRD](vats.md) (`VATS-UI-007`, `VATS-FO4-002`), so its GUI-REG-004
  entry carries no pause and `ui.menus` reports no pause on its account.
- **GUI-REG-011.** Papyrus `Utility.IsInMenuMode` (Skyrim, Fallout 4) MUST
  answer `true` exactly while the registry holds a menu whose flags pause
  the game, so that an open dialogue menu answers `false`, as the CK and
  Fallout 4 CK define it (§2 *Menu mode*). Today it has no handler (§3.1
  row 17).

### 5.2 Host-method behaviour (`GUI-HOST`)

- **GUI-HOST-001.** Every catalog entry of every in-scope profile MUST
  carry one behaviour class (H1–H4, §3.2) in the catalog itself
  (`crates/ui/src/catalog.rs`), next to the existing `provenance` field.
  Today a call's `ScaleformHostDispatch` (`crates/ui/src/host.rs`) records
  transport state only.
- **GUI-HOST-002.** An H1 method MUST act through the same canonical API the
  native page for that menu uses (inventory, equipment, barter, crafting,
  dialogue, settings, save), so there is one validation path per action;
  the observable is one test per H1 action that drives the action once
  through the host method and once through the native page's control and
  asserts the same resulting ECS state. The native pages already follow
  this rule for settings ("one validation/application path",
  `draw_game_menu` doc in `crates/debug-ui/src/panels.rs`).
- **GUI-HOST-003.** An H2 answer MUST name its reason in the catalog, and
  methods whose purpose is an online service MUST be H2 with the reason
  "anti-scope: hosted services" (GUI-NG-003). **Phase 0: verify** which
  vanilla methods those are, settled by the per-method review of the Phase 0
  catalog against the menu each method is called from.
- **GUI-HOST-004.** An H3 or H4 call MUST produce one attributed diagnostic
  per (menu, method) naming the class, as the drain does today for
  `Unknown` / `MissingResponse` (§3.1 row 4), and MUST NOT be answered with
  a fabricated value. For a mod's replacement movie the H4 half of this is
  `LMC-UI-001`'s attributed diagnostic, cited here and not restated; this
  ID adds H3 and covers vanilla movies.
- **GUI-HOST-005.** A response handler MUST answer from canonical state on
  the frame it is asked; the Scaleform HUD driver's poll handlers
  (`byroredux/src/scaleform_hud.rs`) are the pattern and MUST move behind the
  general handler table rather than staying HUD-private.
- **GUI-HOST-006.** Per-game differences in host protocol (Skyrim's
  re-entrant `GameDelegate` request IDs, read off SkyUI's
  [`GameDelegate.as`](https://github.com/schlangster/skyui/blob/master/src/CLIK/gfx/io/GameDelegate.as);
  Fallout 4's `BGSCodeObj` function object, read off the reconstructed
  [F4CF interface source](https://github.com/F4CF/Interface))
  MUST stay inside the host profile; a handler is written once against the
  logical method and the canonical state it reads.

### 5.3 Path-addressed access and adapter injection (`GUI-PATH`)

- **GUI-PATH-001.** The engine MUST offer path-addressed get, set and
  invoke on any open movie of either VM, through adapter injection (Q2).
  Today Ruffle's `call_internal_interface` reaches registered
  ExternalInterface callbacks only, which is why the vanilla meters stay
  empty ([ui.md](../engine/ui.md#scaleform-hud-m486-skyrim--m487-fallout-4)).
- **GUI-PATH-002.** A set or invoke on a path that does not exist in the
  movie MUST return a typed "absent path" result and a diagnostic, never a
  silent no-op. This mirrors Scaleform's own contract: `Movie::Invoke`
  returns false when the method was not invoked, and calling a routine
  whose frame has not yet played prints an error to the Scaleform log
  ([Autodesk: ActionScript](https://help.autodesk.com/cloudhelp/ENU/Scaleform-Help/scaleform_help/game_communication/c_actionscript.html)).
- **GUI-PATH-003.** A path operation issued before the movie's target exists
  MUST be either queued until the target appears or refused with the
  absent-path result; the choice is per call and stated in the call.
- **GUI-PATH-004.** Injected adapter members MUST use the `__byro` name
  prefix the AVM2 adapter already uses (`crates/ui/src/avm2_host.rs:26-33`),
  and every one whose absence is functional MUST be listed by exact name in
  `RESERVED_ENGINE_CALLBACKS` (`crates/ui/src/host.rs:87`). The prefix
  alone does not stop a movie registering the same names (`host.rs:69-76`
  records a movie occupying a prefix-keyed band before the adapter ran,
  #3434); exact-name reservation is the guarantee the bridge keeps today.
- **GUI-PATH-005.** The path service MUST be the only engine code that
  reads, writes or invokes a movie member by path; any SKSE / F4SE `UI`
  function `LMC` maps (SKSE `UI.SetFloat` / `Invoke*` take a menu name and
  a `_root` / `_Global` target, [CK: UI Script](https://ck.uesp.net/wiki/UI_Script);
  F4SE `UI.Get` / `Set` / `Invoke`, [FO4 CK: UI Script](https://falloutck.uesp.net/wiki/UI_Script))
  reaches a movie through it and through no second route. This fixes the
  route only: it grants scripts no path access. The SDK plan already
  decided that Scaleform access is replaced by a versioned UI contribution
  service with isolated data and actions
  ([SDK plan §7](../engine/sdk-v0.1-development-plan.md#7-script-extender-replacement-map)),
  and today every `UI` function but the read-only `IsMenuOpen` is
  classified unsupported pending that service
  (`classify_static_call`, `crates/sdk/src/compatibility/source_alias.rs:451-462`);
  which functions are served, and behind which capability, is `LMC`'s
  disposition (its catalog row S4) under that design.

### 5.4 HUD (`GUI-HUD`)

- **GUI-HUD-001.** With `--hud` on Skyrim, the vanilla `hudmenu.swf` health,
  magicka and stamina meters MUST follow the player's composed actor values;
  `hud.values 1.0` then `0.35` MUST change the meter band's pixels and leave
  a control band unchanged. Today they stay empty (§3.1 row 6).
- **GUI-HUD-002.** The Fallout 4 twin of GUI-HUD-001 MUST hold for the
  health and action-point meters.
- **GUI-HUD-003.** The vanilla meter feed paths MUST be the ones the
  original engine drives. **Phase 0: verify** them, settled by decompiling
  each `hudmenu.swf` with JPEXS FFDec (the tool the CK wiki documents,
  [FO4 CK: JPEXS](https://falloutck.uesp.net/wiki/JPEXS_Free_Flash_Decompiler_(FFDec)))
  and listing the meter-setting members no ActionScript in the movie calls
  itself; the list is checked in as data beside the catalog.
- **GUI-HUD-004.** When an original HUD drives the player's vitals, the
  native HUD's vitals bars MUST NOT draw a second copy; today they are gated
  only by `interface.show_vitals` (§3.1 row 3).
- **GUI-HUD-005.** The compass of each original HUD MUST follow the camera
  heading, as the MenuXml HUDs already do through `cropx`
  ([ui.md](../engine/ui.md#menuxml-legacy-ui-m484-oblivion--m485-fo3)).
  Today the vanilla Skyrim and Fallout 4 compasses are not driven (§3.1
  row 6); their feed paths are part of GUI-HUD-003's verify.
- **GUI-HUD-006.** The FO3 / FNV HUD art SHOULD be tinted by the game's HUD
  colour instead of rendering art-native white: ui.md records that the
  `systemcolor &hudmain;` default is not pinned. **Phase 0: verify** the
  default and its INI source, settled by the GECK INI pages and the shipped
  `Fallout_default.ini`.

### 5.5 Scaleform extensions (`GUI-GFX`)

- **GUI-GFX-001.** The AVM1 profile MUST provide the Scaleform AS2 extension
  members vanilla movies read, enabled the way Scaleform enables them
  (`_global.gfxExtensions = true`,
  [Autodesk: AS2 Extensions Reference](https://help.autodesk.com/cloudhelp/ENU/Scaleform-Help/scaleform_help/as2_reference.html)).
  Today none exist (§3.1 row 7).
- **GUI-GFX-002.** The AVM2 profile MUST provide the AS3 extension surface
  (`scaleform.gfx.Extensions`,
  [Autodesk: AS3 Extensions Reference](https://help.autodesk.com/cloudhelp/ENU/Scaleform-Help/scaleform_help/as3_reference.html))
  that vanilla Fallout 4 movies read.
- **GUI-GFX-003.** Phase 0 MUST measure, per game, which extension members
  the vanilla interface archive references, with the scanners that already
  measure host calls (`crates/ui/src/avm1_host.rs`,
  `crates/ui/src/avm2_host.rs`); only measured members are built, and each
  is H1 or H2 by the same rule as host methods.
- **GUI-GFX-004.** An extension member a movie reads that the profile lacks
  MUST produce one diagnostic per (menu, member).

### 5.6 Fonts and text (`GUI-FONT`)

- **GUI-FONT-001.** Each Scaleform profile MUST resolve font classes from
  the game's font configuration to the font libraries it names (Fallout 4:
  `Interface\FontConfig.txt` mapping classes such as `$MAIN_Font` to
  `Interface\fonts_en.swf`,
  [FO4 CK: Text Replacement § Fonts](https://falloutck.uesp.net/wiki/Text_Replacement)).
  **Phase 0: verify** the Skyrim file name and format, settled by listing
  `Skyrim - Interface.bsa` and reading the file.
- **GUI-FONT-002.** The font library MUST be the one the active language
  installs: Skyrim ships one `fonts_en.swf` name for every language, with
  the encoding decided by the file ([UESP: String Table File Format § String
  Encodings](https://en.uesp.net/wiki/Skyrim_Mod:String_Table_File_Format)).
- **GUI-FONT-003.** A font class or glyph that does not resolve MUST fall
  back to the font Q5 chooses, with one diagnostic per (menu, class).
- **GUI-FONT-004.** Scaleform `$`-token text replacement MUST resolve from
  the game's vanilla translation tables. **Phase 0: verify** the table paths
  and format per game, settled by listing each interface archive; mod
  tables are `LMC-UI-004`.
- **GUI-FONT-005.** FO3 / FNV MenuXml `-s`-prefixed strings SHOULD resolve
  through the plugin's game settings. **Phase 0: verify** that they are
  game-setting references, settled by a census of the prefixed names in
  the XML corpus against the `GMST` editor IDs in `FalloutNV.esm` and
  `Fallout3.esm`.

### 5.7 Script and engine events (`GUI-EVT`)

- **GUI-EVT-001.** Opening or closing a menu in the registry MUST raise
  Fallout 4's `OnMenuOpenCloseEvent(asMenuName, abOpening)` on every script
  registered through `RegisterForMenuOpenCloseEvent` for that menu name,
  vanilla Papyrus on Fallout 4
  ([FO4 CK: OnMenuOpenCloseEvent](https://falloutck.uesp.net/wiki/OnMenuOpenCloseEvent_-_ScriptObject)).
  Today nothing handles either (§3.1 row 11).
- **GUI-EVT-002.** The registration MUST unregister automatically when the
  owning quest stops or the active magic effect is removed, as the CK page
  documents.
- **GUI-EVT-003.** Script functions that open a menu MUST open it through
  the registry, under the game-facing name the per-game inventory gives
  (not today's engine-coined `MenuRequest` names, §3.1 row 13), with the
  surface the inventory names, or report S4 with the fallback the effect
  already uses (the auto-accept placeholder, or the `script.requests` log
  for the ObScript requests nothing opens). The Phase 0 inventory
  (GUI-REG-006) lists them per game from the cited pages (examples:
  Skyrim `ShowBarterMenu`, `ShowGiftMenu`
  ([CK: ShowGiftMenu](https://ck.uesp.net/wiki/ShowGiftMenu_-_Actor)),
  `ShowTrainingMenu`, `ShowRaceMenu`, `ShowLimitedRaceMenu`
  ([CK: ShowLimitedRaceMenu](https://ck.uesp.net/wiki/ShowLimitedRaceMenu_-_Game)); FO3 / FNV `ShowBarberMenu`,
  `ShowSleepWaitMenu`, `ShowNameMenu`, `ShowRaceMenu`,
  [GECK: ShowRaceMenu](https://geck.uesp.net/wiki/ShowRaceMenu); Oblivion
  `ShowClassMenu`, `ShowBirthsignMenu`, `ShowRaceMenu`,
  [CS: ShowRaceMenu](https://cs.uesp.net/wiki/ShowRaceMenu)).
- **GUI-EVT-004.** A host call that changes game state (an equip, a trade,
  a topic choice) MUST reach the ECS as the same event or command the
  native page emits for that action, applied after the UI frame, as the
  native pages apply theirs today ([ui.md](../engine/ui.md#input-routing)).
- **GUI-EVT-005.** A menu open MUST NOT change saveable state by itself;
  the registry is presentation state, republished every frame, the same
  `NOT_SAVED_BY_DESIGN` rule `UiMenuState` carries today
  (`byroredux/src/systems/script_queries.rs:13-14`).
- **GUI-EVT-006.** ObScript `Begin MenuMode [code]` blocks MUST run on the
  frames the registry holds a menu matching the block's code (any
  menu-mode menu when it names none), and `Begin GameMode` blocks MUST NOT
  run on those frames, as the GECK and CS block pages define them (§2
  *Menu mode*), even while GUI-REG-010 has the simulation paused. Which
  registry menus count toward menu mode is a per-menu entry of the
  GUI-REG-004 table; a HUD menu does not, since GameMode blocks run "during
  normal gameplay" while the HUD is up ([GECK: GameMode](https://geck.uesp.net/wiki/GameMode)).
  Today no `MenuMode` block runs
  (§3.1 row 17). **Phase 0: verify** the compiled block-type number of
  `Begin MenuMode` per game, settled by the `obscript_tools census`
  block-name alignment the VM already uses for its other block numbers
  (the VM's test fixture labels type 9 `MenuMode`, but the census assigns
  9 to `OnHitWith`, `crates/scripting/src/obscript_vm.rs:88-93`).

### 5.8 Native pages and residues (`GUI-PAGE`)

- **GUI-PAGE-001.** Every native page MUST register in the registry under
  the game-facing name of the menu it stands in for (`InventoryMenu`,
  `Dialog`, code `1009`, …), so that scripts see it as that menu.
- **GUI-PAGE-002.** `Game.ShowRaceMenu()` / ObScript `ShowRaceMenu` MUST open
  an interactive race and sex selection on the surface Q4 chooses, listed
  open in the registry until the player accepts. Today the call is a
  placeholder that auto-accepts (§3.1 row 12). **Phase 0: verify** what the
  caller observes while the menu is open — whether the original native
  returns before the menu closes, and which quest state advances on accept
  — settled by the MQ101 fragment and scene that follow the call
  (decompiled with the repository's `.pex` reader) and, for FNV, the result
  script after the first `ShowRaceMenu`. The CK and GECK pages
  ([CK: ShowRaceMenu](https://ck.uesp.net/wiki/ShowRaceMenu_-_Game),
  [GECK: ShowRaceMenu](https://geck.uesp.net/wiki/ShowRaceMenu)) do not
  say. The CS page ([CS: ShowRaceMenu](https://cs.uesp.net/wiki/ShowRaceMenu))
  warns that calling more than one character-creation menu at a time
  freezes controls and crashes and that scripts must pause between the
  calls, which is consistent with Oblivion's call returning while its menu
  is still open; the verify confirms that per game rather than assuming
  it.
- **GUI-PAGE-003.** The Fallout 4 `ShowRaceMenu` parameters (`uiMode` 0–4,
  the menu target and vendor, [FO4 CK: ShowRaceMenu](https://falloutck.uesp.net/wiki/ShowRaceMenu_-_Game))
  MUST lower; today only the zero-argument form lowers (§3.1 row 12).
  Modes other than start-of-game chargen MAY report S4 until the slider
  milestone (GUI-NG-005).
- **GUI-PAGE-004.** The dialogue menu MUST reach the surface Q1 chooses
  while keeping every behaviour the native page carries today (speaking
  cue, subtitle, replies, said-markers, [Dialogue Trees §3](../engine/dialogue-trees.md#3-scope));
  on FNV `dt1-dialogue-layers.sh` Phase P MUST pass on that surface. That
  script runs FNV only (FO3 is a declared skip), so the Skyrim observable
  is the same Phase P leg (speaker named, list mode, speaking cue) added to
  `dt2-skyrim-forcegreet.sh`, which already opens a Skyrim conversation
  headless and polls `dialogue.status`.
- **GUI-PAGE-005.** Where a native page remains the surface, its layout MAY
  differ from the original menu (GUI-NG-008), but every label it shows
  MUST resolve from the game's data (a record's name or a `GMST` string)
  unless the Phase 0 inventory lists it as an engine string with its
  reason; a per-page test asserts that no unlisted engine string is drawn.
- **GUI-PAGE-006.** Player-facing native pages SHOULD live outside the
  operator overlay crate; today they are in `byroredux-debug-ui`
  (`crates/debug-ui/src/panels.rs`) beside the F3 operator panels.

### 5.9 MenuXml menus beyond the HUD (`GUI-XML`)

- **GUI-XML-001.** The MenuXml track MUST load any menu document of the
  three pre-Skyrim games through the same `MenuProfile`, not only
  `hud_main_menu.xml` (§3.1 row 15); which menus become S2 follows Q1.
- **GUI-XML-002.** MenuXml input (pointer hit-testing over tile rects,
  keyboard navigation) MUST route through the same `UiInputEvent` contract
  the Scaleform track uses (`crates/ui/src/input.rs`), so that one input
  owner rule (GUI-REG-009) covers both tracks.
- **GUI-XML-003.** Values a MenuXml menu reads from the engine MUST be set
  through the override API the HUD uses (`MenuRenderer::set_override`,
  `instantiate_template`, `graft_fragment` in `crates/menuxml/src/menu.rs`),
  so that the per-game difference stays in the profile.

### 5.10 Observability and cost (`GUI-OPS`)

- **GUI-OPS-001.** A `ui.menus` console command MUST list every registry
  entry: name, surface, flags, input owner, and the open-movie device
  count and pause state. It joins `hud.status` / `hud.debug`, which stay.
- **GUI-OPS-002.** A `ui.host <menu>` console command MUST report, for one
  open menu, the count of calls per behaviour class and the distinct H3 /
  H4 method names, bounded the way the drain's diagnostic set is bounded
  today (`MAX_DISTINCT_HOST_METHOD_NAMES`, defined `crates/ui/src/host.rs:65`,
  applied `byroredux/src/app_frame.rs:1463`).
- **GUI-OPS-003.** UI cost per frame (Ruffle tick + render + readback +
  upload, plus MenuXml raster) MUST be measurable per open menu and
  published under the ROADMAP bench methodology
  ([How to run a bench](../../ROADMAP.md#how-to-run-a-bench)): same-machine
  control, pinned camera.
- **GUI-OPS-004.** A static menu SHOULD cost no raster work, the rule the
  HUD drivers already keep with a change signature and a 33 ms cadence cap
  ([ui.md](../engine/ui.md#menuxml-legacy-ui-m484-oblivion--m485-fo3)).
- **GUI-OPS-005.** A `ui.open <menu>` / `ui.close <menu> [accept|cancel]`
  console pair MUST open a menu by its game-facing name through the
  registry and close it with the stated result, so §7.2's scenarios drive
  menus headless; the next `ui.menus` line MUST show the change. Today
  each menu has its own driver and none names a menu by its game-facing
  name: `input.press inventory` (`debug_action`,
  `byroredux/src/interaction.rs:665-676`), `ui.answer` for a waiting
  message box and `dialogue.select` / `dialogue.end` for the dialogue page
  (`byroredux/src/commands/quest.rs`), and `hud.on` / `hud.off` for the HUD
  (`byroredux/src/commands/hud.rs`).

---

## 6. Deliberate deviations and risks

| # | Original behaviour | Redux behaviour | Becomes redundant | Risk and its requirement |
|---|---|---|---|---|
| 1 | Native code can drive menus by path through GFx `Invoke` / `SetVariable` ([Autodesk](https://help.autodesk.com/cloudhelp/ENU/Scaleform-Help/scaleform_help/game_communication.html)), the operation SKSE's `UI.Set*` / `Invoke*` expose by menu name and `_root` path ([CK: UI Script](https://ck.uesp.net/wiki/UI_Script)); which paths the vanilla HUDs are fed through is GUI-HUD-003's verify | A bytecode adapter injected at load serves path access through ExternalInterface (GUI-PATH-001) | The SkyUI-class poll handlers as the only meter feed (they stay, behind the general table, GUI-HOST-005) | A movie that rebuilds its root after load loses the adapter → GUI-PATH-002 reports the absent path; GUI-PATH-004 keeps names apart |
| 2 | Every menu is the game's own movie or document | Some menus are native pages (Q1) | Nothing | Mod movies for those menus do not run → GUI-PAGE-001 reports the surface, `LMC-UI-001` reports the mod as affected |
| 3 | `ShowRaceMenu` opens a menu that also edits the face and hair ([GECK: ShowRaceMenu](https://geck.uesp.net/wiki/ShowRaceMenu); Fallout 4's modes include haircut, plastic surgery and face paint, [FO4 CK: ShowRaceMenu](https://falloutck.uesp.net/wiki/ShowRaceMenu_-_Game)) | Race/sex selection only (GUI-PAGE-002, Q4) | The auto-accept placeholder `Effect::ShowRaceMenu` | Appearance cannot be edited → GUI-PAGE-003 reports the other modes S4 |
| 4 | Online and storefront menus reach a hosted store ([UESP: Creation Club](https://en.uesp.net/wiki/Skyrim:Creation_Club)) | H2 placeholder, anti-scope (GUI-HOST-003) | Nothing | A menu waits on an answer that never comes → H2 answers immediately with the offline value |
| 5 | Lockpicking / hacking open a minigame menu (GECK *MenuTypes* 1014 LockPick and 1055 Hacking; `LockpickingMenu` in [FO4 CK: Menu](https://falloutck.uesp.net/wiki/Menu)) | No menu; engine policy (`lockpick.rs`) | Nothing | `OnMenuOpenCloseEvent("LockpickingMenu")` never fires → GUI-NG-004 reports the menu S4 |
| 6 | One font set per installed language | Missing class or glyph falls back (GUI-FONT-003, Q5) | Nothing | Visible font mismatch → the diagnostic names the class |
| 7 | Menus share one GFx runtime | Movies share one wgpu device (GUI-REG-005) | The per-`SwfPlayer` wgpu instance and device | None: an engine-internal change; device count is reported |
| 8 | Each menu's flags are the game's own | Values no source documents get a stated engine default (GUI-REG-004) | Nothing | A menu pauses, or allows saving, where the original did not → the flags table names the defaulted entries |

---

## 7. Per-game scope and acceptance

### 7.1 Per-game scope

Waves follow the two playable routes (Skyrim, then FNV as the second
route, [ROADMAP](../../ROADMAP.md#playable-vertical-slice)), one per UI
track, the same first wave the Legacy Mod Compatibility PRD recommends
(its Q4); after them, the UI track a game shares outranks its engine tier
in [game-compatibility.md](../engine/game-compatibility.md#per-game-support-detail),
because the machinery is per track.

| Game | In scope | Why / wave |
|---|---|---|
| Oblivion | yes | Wave 2, ahead of its engine Tier 3 because it shares FNV's MenuXml machinery and its HUD already ships (M48.4) |
| Fallout 3 | yes | Wave 2. MenuXml track, HUD shipped (M48.5); shares FNV's profile, so its MenuXml menus (GUI-XML) land with FNV's in Phase 3, which the [VATS PRD](vats.md) depends on |
| Fallout: New Vegas | yes | Wave 1. Reference title and second playable route; MenuXml track |
| Skyrim (LE / SE / AE) | yes | Wave 1. First playable route; AVM1 profile and the SkyUI-class protocols, measured on SE. LE is parse-only in the [compatibility matrix](../../ROADMAP.md#compatibility-matrix), so its live scenarios wait for LE cells to load; **Phase 0: verify** that LE's interface archive needs no profile change, settled by running the existing AVM1 sweep on the LE install and diffing its host-call inventory against SE's |
| Fallout 4 | yes | Wave 2. AVM2 profile and adapter shipped (M48.7) |
| Fallout 76 | no | Tier 3, parse only: no cell loads, so no HUD or menu has a world to sit over ([game-compatibility.md](../engine/game-compatibility.md#fallout-76)); revisit when FO76 cells load |
| Starfield | yes, last | Wave 3, Tier 2. `hudmenu.swf` parses through the PlaceObject3 shim (#4470, titled M48.8), but `ScaleformProfile` has only the Skyrim and Fallout 4 variants (`crates/ui/src/profile.rs:12-17`) and #4470 is closed with the rest of that bring-up (profile, catalog, driver bars, protocol pin, `m48-8` smoke) unbuilt and untracked. **Phase 0: verify** its host contract, settled by running the AVM2 inventory scanner over `Starfield - Interface.ba2` and recording whether it uses a `BGSCodeObj`-shaped root |

No requirement adds a per-game branch outside the host profiles, the
`MenuProfile` / `HudGameProfile` tables and the per-game menu inventory;
those are this layer's translate step (GUI-HOST-006, GUI-XML-003).

### 7.2 Acceptance

Data: installed vanilla archives only (`Skyrim - Interface.bsa`,
`Fallout4 - Interface.ba2`, `Fallout - Misc.bsa`, `Oblivion - Misc.bsa`,
their texture archives); nothing proprietary is checked in, the rule the
ignored corpus tests already follow ([ui.md](../engine/ui.md#tests)).

A scenario passes when:

1. **Regression floor.** [`m48-4-oblivion-hud.sh`](../smoke-tests/m48-4-oblivion-hud.sh),
   [`m48-5-fo3-hud.sh`](../smoke-tests/m48-5-fo3-hud.sh),
   [`m48-5-fnv-hud.sh`](../smoke-tests/m48-5-fnv-hud.sh),
   [`m48-6-skyrim-hud.sh`](../smoke-tests/m48-6-skyrim-hud.sh),
   [`m48-7-fo4-hud.sh`](../smoke-tests/m48-7-fo4-hud.sh),
   [`m48-menu-load.sh`](../smoke-tests/m48-menu-load.sh),
   [`p3-hud.sh`](../smoke-tests/p3-hud.sh) (Skyrim and `fnv`),
   [`dt1-dialogue-layers.sh`](../smoke-tests/dt1-dialogue-layers.sh) and
   [`m43-crafting.sh`](../smoke-tests/m43-crafting.sh) pass unchanged at
   every phase exit.
2. **Registry agreement.** New `cargo test -p byroredux --bin byroredux`
   test `menu_registry_answers_is_menu_open_and_menu_mode_alike`, on
   registry entries (no live movie, so no GPU): with a native Inventory
   page whose flags pause and a Scaleform HUD registered, the Skyrim table answers
   `UI.IsMenuOpen("InventoryMenu")` and `UI.IsMenuOpen("HUD Menu")` true,
   the FNV table answers `MenuMode 1002` true, and `Utility.IsInMenuMode`
   is true; with only a Dialogue entry it is false (GUI-REG-001/002/003/011).
3. **Stack over a live HUD.** New script `m48-menu-stack.sh` (Skyrim SE,
   BleakFallsBarrow01, `--hud`): `--bench-hold`, then `byro-dbg` runs
   `ui.menus`, `input.press inventory`, `ui.open MessageBoxMenu`,
   `ui.close MessageBoxMenu cancel`; each `ui.menus` line shows the HUD
   open, the expected input owner, and a device count of 1
   (GUI-REG-003/005/007/009, GUI-OPS-001/005).
4. **Live Skyrim meters.** New script `m48-skyrim-hud-meters.sh` (not
   `m48-8`, which #4470 and the UI audits reserve for Starfield's HUD),
   same fixture as `m48-6`: `hud.values 1.0 1.0 1.0`, capture,
   `hud.values 0.35 1.0 1.0`, capture; the health band's changed-pixel
   count is above zero and a control band's is zero (GUI-HUD-001).
5. **Live Fallout 4 meters.** New script `m48-fo4-hud-meters.sh` on
   MedTekResearch01, the same shape for health and AP (GUI-HUD-002).
6. **Path access.** New `cargo test -p byroredux-ui` tests
   `avm1_adapter_sets_and_reads_a_root_path` and
   `avm2_adapter_reports_an_absent_path`, on synthetic movies built the way
   the existing Ruffle-fixture tests build theirs (GUI-PATH-001/002).
7. **Behaviour coverage.** An ignored corpus test per wave-1 Scaleform game
   (pattern: `installed_skyrim_host_calls_are_all_cataloged`) asserts every
   method the vanilla archive calls has a class and no H4 remains
   (GUI-HOST-001); `ui.host` reports the same counts live (GUI-OPS-002).
8. **Menu events.** New script `m48-menu-events.sh` (Fallout 4): a vanilla
   script registered for a menu by `RegisterForMenuOpenCloseEvent`, chosen
   in Phase 0 by a census of `Fallout4 - Misc.ba2` scripts, receives one
   opening and one closing event (GUI-EVT-001).
9. **Race menu.** New script `m48-race-menu.sh` (Skyrim SE, MQ101 started
   at the stage whose fragment calls `Game.ShowRaceMenu()`, located in
   Phase 0 by the `mq101_conformance` example's fragment report): `ui.menus`
   lists `RaceSex Menu` open; `ui.close "RaceSex Menu" accept` closes it and
   the quest state Phase 0's GUI-PAGE-002 verify names advances. FNV twin:
   the first `ShowRaceMenu` call of the opening sequence, located in Phase 0
   by a census of `FalloutNV.esm` result scripts (GUI-PAGE-002,
   GUI-OPS-005).
10. **Menu mode and pause.** New `cargo test -p byroredux --bin byroredux`
    test `menu_mode_blocks_and_pause_follow_the_registry`, on registry
    entries and a synthetic ObScript object script with one `Begin
    MenuMode 1002` and one `Begin GameMode` block, on the FNV tables: with
    an Inventory entry whose flags pause, the simulation is paused, the
    MenuMode block runs and the GameMode block does not; with only the HUD
    entry, the simulation advances, the GameMode block runs and the
    MenuMode block does not (GUI-REG-010, GUI-EVT-006).

---

## 8. Metrics

One home for the numbers: the ROADMAP M48 row and, per game, the
[compatibility matrix](../../ROADMAP.md#compatibility-matrix). The Legacy
Mod Compatibility PRD's *UI method coverage* counts methods called by
*mod* movies; the coverage below counts vanilla movies only, so the two
never share a denominator.

| Metric | Definition | Granularity | ROADMAP home |
|---|---|---|---|
| Menu surface coverage | menus with surface S1–S3 ÷ menus in the game's Phase 0 inventory | per game, per surface class | compatibility matrix, per game |
| Host behaviour coverage | H1 methods ÷ catalog methods the vanilla interface archive calls | per game | compatibility matrix, per game |
| Placeholder share | H2 methods ÷ catalog methods the vanilla interface archive calls | per game | M48 row |
| HUD meter liveness | meters whose pixel gate passes ÷ meters the vanilla HUD shows | per game | M48 row |
| UI frame cost | main-thread ms per frame for UI tick, render, readback and upload, at a pinned camera | per game, per open-menu count | M48 row, under the bench methodology |

---

## 9. Delivery sequence

Phases are ordered by what they unblock: the registry first, because every
later requirement reports through it; the HUD meters second, because they
prove the path service on both VMs; then behaviour, then surfaces.

**Phase 0 — Inventory, catalog classes, design (no engine code).**
Write the menu registry / stack / path-access design as a section of
[ui.md](../engine/ui.md#input-routing) (GUI-REG, GUI-PATH). Check in the
per-game menu inventory (GUI-REG-006) and the flags table sources
(GUI-REG-004). Classify every catalog entry H1–H4 as a target (GUI-HOST-001,
GUI-HOST-003). Measure extension members (GUI-GFX-003). Close
every **Phase 0: verify** item: what the original reports and draws for a
HUD under a modal (GUI-REG-003), every vanilla menu's flags
(GUI-REG-004), FO3 / FNV code 1004 (GUI-REG-006), the compiled
`Begin MenuMode` block number (GUI-EVT-006), the
online-service methods (GUI-HOST-003), the HUD feed paths (GUI-HUD-003),
the `&hudmain;` default (GUI-HUD-006), Skyrim's font configuration
(GUI-FONT-001), translation tables (GUI-FONT-004), the `-s` strings
(GUI-FONT-005), the race menu's caller-visible wait (GUI-PAGE-002), Skyrim
LE and Starfield (§7.1). Resolve the §10 decisions.
Exit: the inventory, the classified catalog, the design section and the
verify answers are checked in, and `python3
scripts/check-prd-conformance.py` passes on the revised PRD.

**Phase 1 — Registry and stack.** GUI-REG-001…011, GUI-PAGE-001,
GUI-EVT-005/006, GUI-OPS-001, GUI-OPS-005. Exit: scenarios 2, 3 and 10
pass, and scenario 1's floor passes.

**Phase 2 — Path service and live HUD meters.** GUI-PATH-001…005,
GUI-HUD-001…005. Exit: scenarios 4, 5 and 6 pass; HUD meter liveness is
published for Skyrim and Fallout 4.

**Phase 3 — Behaviour, extensions and text, wave 1.** GUI-HOST-001…006,
GUI-GFX-001/003/004, GUI-FONT-001…004, GUI-EVT-003/004 for Skyrim;
GUI-XML-001…003 and GUI-FONT-005 for FNV and, through the shared Fallout
`MenuProfile`, FO3 (the [VATS PRD](vats.md)'s menu phase needs
GUI-XML-001…003 on both). Exit: scenario 7 passes for Skyrim; a
non-HUD FNV and FO3 menu document loads and takes pointer input through
`UiInputEvent` in a `cargo test` the Phase 0 design names (corpus-gated
like the existing `fo3_corpus.rs`); host behaviour
coverage, placeholder share and UI frame cost are published for Skyrim and
FNV.

**Phase 4 — Surfaces per Q1, wave 1.** The dialogue menu first
(GUI-PAGE-004), then the menus Q1 assigns, on Skyrim and FNV;
GUI-PAGE-005/006. Exit: Phase P passes on the chosen surface on both
games (`dt1-dialogue-layers.sh` on FNV, the new Phase P leg of
`dt2-skyrim-forcegreet.sh` on Skyrim, GUI-PAGE-004), and menu surface
coverage is published for both.

**Phase 5 — Race menu.** GUI-PAGE-002/003 per Q4. Exit: scenario 9 passes
on Skyrim SE and FNV, and the MQ101 Known Issue is closed in ROADMAP.

**Phase 6 — Wave 2 and Starfield.** Fallout 4 (GUI-GFX-002, GUI-EVT-001/002),
Fallout 3, Oblivion; then Starfield's profile and catalog if Phase 0's
verify found a host contract. Exit: scenario 8 passes, and every metric of
§8 is published for every in-scope game.

Dependencies: M43's dialogue layers (closed) for Phase 4; M47.2 / M47.3 for
the script queries and blocks of Phase 1 (`MenuMode`, `Begin MenuMode`,
`Utility.IsInMenuMode`) and the script events of Phase 6; the legacy-mod
PRD's Phase 3 and the SDK plan's UI contribution service for the extender
UI functions GUI-PATH-005 routes. Dependents: the [VATS PRD](vats.md)'s
menu phase needs Phase 1 (`GUI-REG`) and Phase 3's `GUI-XML-001`…`003` on
FNV and FO3.

---

## 10. Decisions required

Each needs the project owner. The recommendation is the first option.

1. **Q1 — Which surface is the product.** Recommended: the game's own
   movie or document (S1 / S2: its `.swf` or MenuXml file running in the
   engine) for every menu whose host calls are all served from engine
   state (H1) for the actions it offers, with the engine-drawn native page
   (S3) kept as the fallback for missing data, unserved menus and
   Starfield, and the dialogue menu first. This is what R4's "preserving SWF mod
   compatibility" buys, and it lets `LMC-UI-001` / `LMC-UI-003` cover
   inventory, dialogue and container mods. Alternative A: native pages are
   the product; original movies run only for the HUD. Consequence: far less
   host behaviour to build, but UI mods for any other menu never run, and
   the dialogue "restyle" becomes a native-page design task. Alternative B:
   original surfaces only, native pages removed. Consequence: the playable
   slice gates that run on native pages (`p3-hud.sh`, `m43-crafting.sh`,
   `dt1-dialogue-layers.sh`) fail until every menu is served.
2. **Q2 — HUD meter route (the decision the ROADMAP row names).**
   Recommended: adapter injection on both VMs, an AVM1 adapter mirroring the
   shipped AVM2 one, serving path-addressed access (GUI-PATH). It keeps the
   vanilla movie and HUD mods intact and leaves the engine one path route,
   which any extender UI function later mapped must use (GUI-PATH-005;
   scripts get no raw path access, since the SDK plan routes Scaleform
   access through its UI contribution service), and it does not reopen R4:
   it is the in-memory bytecode adapter pattern R4's Fallout 4 slice
   already uses, through Ruffle's supported ExternalInterface boundary.
   Whether an AVM1 adapter can reach `_root` members that way is the
   Phase 0 design section's first question; if it cannot, Skyrim falls
   back to Alternative A and the decision returns to the owner.
   Alternative A: SkyUI-class menus only, using the poll protocol the
   driver already answers. Consequence: vanilla Skyrim meters stay empty
   without a mod, and Fallout 4 has no such protocol (its catalog has no
   meter-shaped queries, [ui.md](../engine/ui.md#scaleform-hud-m486-skyrim--m487-fallout-4)).
   Alternative B: extend Ruffle with an object-path API. Consequence:
   reopens R4, which chose not to expose Ruffle's private VM internals, and
   withdraws GUI-NG-001; listed so the owner sees the route R4 closed.
   Alternative C: native vitals over the vanilla chrome. Consequence: two
   HUDs on screen and HUD mods that restyle meters have no effect.
3. **Q3 — One movie per open menu or one modal movie.** Recommended: one
   Ruffle player per open menu on a shared device (GUI-REG-005), every
   visible layer rendered, only the top input owner ticking input. It is the
   only model in which a message box draws over an inventory and the HUD
   stays live. Alternative: at most one modal movie plus the HUD.
   Consequence: cheaper, but a menu that opens another (a message box from
   inventory) replaces it, which changes what scripts observe.
4. **Q4 — Race menu scope.** Recommended: an interactive race and sex
   selection (plus name where the game's menu has it), on the original
   menu where Q1 allows and a native page otherwise, that stays open in
   the registry until the player accepts; whether the calling script waits
   for it or carries on follows what GUI-PAGE-002's Phase 0 check finds per
   game (the Oblivion CS page's warning against calling two
   character-creation menus back to back is consistent with the call not
   waiting), so this decision fixes the scope, not the wait. Sliders
   become a new milestone (T1). Alternative A: keep the
   auto-accept and record it in §6 as an accepted deviation, closing the
   Known Issue by policy. Consequence: no work, but chargen remains a
   placeholder on every game. Alternative B: full slider character creation
   inside M48. Consequence: pulls a morph runtime into the UI milestone and
   delays every other phase.
5. **Q5 — Font fallback.** Recommended: a bundled, redistributable
   open-licence font family chosen in Phase 0, so captures are deterministic
   and pixel gates stay stable on every Linux install. Alternative: the
   system font configuration. Consequence: no bundled file, but glyph
   metrics vary by distribution, and a capture-based gate can pass on one
   machine and fail on another.
6. **Q6 — Starfield in M48.** Context: #4470 (the Starfield HUD, M48.8)
   is closed with only its parser shim built; the profile, catalog, HUD
   bars, protocol pin and smoke it listed have no tracking issue today.
   Recommended: Wave 3 inside M48 if the Phase 0 scan finds a
   `BGSCodeObj`-shaped contract the AVM2 profile can extend, which gives
   that remaining bring-up a home in Phase 6; otherwise out of M48 with a
   reason. Alternative: out of M48 unconditionally until Starfield leaves
   Tier 2. Consequence: the PlaceObject3 shim stays the only Starfield UI
   work and the rest of #4470's list stays unowned until a later
   milestone takes it.

---

## 11. Document map

| Need | Go to |
|---|---|
| Host bridge, profiles, catalogs, adapter, navigator, tests | [ui.md](../engine/ui.md) |
| What the vanilla HUDs call and why the meters are empty | [ui.md § Scaleform HUD](../engine/ui.md#scaleform-hud-m486-skyrim--m487-fallout-4) |
| MenuXml evaluator, layout, raster, `MenuProfile` | [ui.md § MenuXml](../engine/ui.md#menuxml-legacy-ui-m484-oblivion--m485-fo3) |
| Input focus and modal capture today | [ui.md § Input routing](../engine/ui.md#input-routing) |
| Loading screens | [ui.md § loading screens](../engine/ui.md#original-game-loading-screens-during-scene-transitions) |
| The native dialogue page's behaviour | [dialogue-trees.md §3](../engine/dialogue-trees.md#3-scope) |
| VATS menu content, and which GUI IDs it depends on | [vats.md](vats.md) |
| Bethesda's menu list and UI script surface | [creation-engine-ui.md](../legacy/creation-engine-ui.md) |
| UI mod dispositions (`LMC-UI-*`) | [legacy-mod-compatibility.md §5.6](legacy-mod-compatibility.md#56-ui-lmc-ui) |
| Extender UI functions, UI contribution service | [sdk-v0.1-development-plan.md §7](../engine/sdk-v0.1-development-plan.md#7-script-extender-replacement-map) |
| R4, the SWF/GFx decision | [roadmap-history.md, Tier 7](../archive/roadmap-history.md#tier-7--deep-gameplay-systems-deferred-until-tier-14-proves-out) |
| Engine tiers per game | [game-compatibility.md](../engine/game-compatibility.md) |
| Smoke scripts and how to run them | [docs/smoke-tests/README.md](../smoke-tests/README.md) |
| Live milestone state and published metrics | [ROADMAP.md](../../ROADMAP.md) |

---

## 12. Revision and review log

| Rev | Date | Change | Review |
|---|---|---|---|
| 1 | 2026-11-01 | First draft | — |
| 2 | 2026-11-01 | Review round 1 folded in: §3.1 rows 7 and 11 evidence corrected at HEAD (row 3 and row 6 tightened); FO4 `MenuData` citation re-read (custom-menu struct, one vanilla value) and GUI-REG-004 made a verify; Scaleform `Invoke` paraphrase corrected (GUI-PATH-002); HUD-under-modal visibility (GUI-REG-003) and the race menu's caller-visible wait (GUI-PAGE-002) moved to **Phase 0: verify**; input-owner rule split out as GUI-REG-009; GECK menu-code gaps (GUI-REG-006); `ui.open` / `ui.close` driver GUI-OPS-005 for scenarios 3 and 9; GUI-HOST-002 observable; `LMC-UI-001` cited in GUI-HOST-004; protocol citations in GUI-HOST-006; VATS PRD linked and FO3 MenuXml menus pulled into Phase 3 for it; §6 redundancy column and flags-default row; §8 ROADMAP homes; Q2 states it does not reopen R4 | claude: revise |
| 3 | 2026-11-03 | Review round 2 folded in: §3.1 re-verified at HEAD with file:line (rows 2, 3, 5, 9, 10, 13, 14 corrected or completed; rows 16 pause and 17 menu-mode script surfaces added); `Utility.IsInMenuMode`, the native pause and `Begin MenuMode` blocks brought into scope (GUI-REG-010, GUI-REG-011, GUI-EVT-006, scenario 10), with the HUD kept out of menu mode (GUI-REG-002/004); S4 gains the `recorded` fallback; menu codes sourced from xEdit `wbMenuModeEnum` and the GECK gap claim corrected (GUI-REG-002/006); GUI-PATH-005 held to the SDK plan's UI contribution service and GUI-PATH-004 to exact-name reservation; GUI-REG-007/008 and GUI-OPS-005 HEAD wording; GUI-XML-002 cross-reference; Skyrim twin for the dialogue Phase P gate (GUI-PAGE-004, Phase 4); GUI-PAGE-005 made checkable; the VATS menu's pause entry cites `VATS-UI-007` (GUI-REG-004/010); CS evidence on the race-menu wait (GUI-PAGE-002, Q4); uncited §6 / GUI-NG-003 claims sourced; §7.1 FO76, Oblivion, Skyrim LE and Starfield (#4470) rows; meter scripts renamed off `m48-8`; Q1, Q2, Q4, Q6 wording | claude: revise |
