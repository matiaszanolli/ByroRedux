# Game UI Integration — Product Requirements

**Status**: ACTIVE (2026-11-27). Revision 12 records the owner's approval of revision 11 (review round 4); the history is §12. Statements about
how an original engine, file format or tool behaves cite external sources
(the Creation Kit / GECK / CS wikis, xEdit's record definitions, Autodesk's
Scaleform documentation, the Ruffle project); a claim that could not be
verified is marked **Phase 0: verify** instead of being stated as fact.

**Tracking**: #6641 (parent). Phases: #6643 (0), #6653 (1), #6661 (2), #6665 (3), #6666 (4), #6667 (5), #6668 (6). Oblivion's sleep/rest and level-up menu contract: #7048, built in #7086.

**Scope**: The player-facing user interface of Oblivion, Fallout 3,
Fallout: New Vegas, Skyrim and Fallout 4: which surface renders each of a
game's menus (the game's own
Scaleform movie, its own MenuXml document, or an engine-native page), how
open menus stack and own input, what the engine does when a menu calls the
host, how the engine and scripts read and drive menu state, fonts and
text, and the residues other milestones handed to M48 (vanilla Skyrim /
Fallout 4 HUD meters, the race menu, the dialogue page's presentation,
and Oblivion's sleep/rest and level-up menus).
Out of scope: UI *mod* compatibility as a promise (owned by the
[Legacy Mod Compatibility PRD](legacy-mod-compatibility.md#56-ui-lmc-ui),
`LMC-UI-*`, which this document serves), the Mod Configuration Menu host
(`LMC-UI-002`), gameplay minigames (lockpicking, hacking), what the VATS
menu shows and which VATS functions its controls call (the
[VATS and Action Points PRD](vats.md), whose menu runs on this document's
registry `GUI-REG` and MenuXml menus `GUI-XML-001`…`003`), Starfield and
Fallout 76 (§7.1).

**Related**: this document states the *promise*; the designs below own the
*how*. Every mechanism promised here has an owner in this table, or Phase 0
writes the missing design.

| Mechanism | Owning design |
|---|---|
| Ruffle host bridge, host profiles, catalogs, AVM2 adapter, navigator, input routing | [UI System](../engine/ui.md) |
| Scaleform HUD route (`--hud`, Skyrim / Fallout 4) | [UI System § Scaleform HUD](../engine/ui.md#scaleform-hud-m486-skyrim--m487-fallout-4) |
| MenuXml track (Oblivion / FO3 / FNV) | [UI System § MenuXml legacy UI](../engine/ui.md#menuxml-legacy-ui-m484-oblivion--m485-fo3) |
| Oblivion sleep completion and level application | [Skill Use § Levels](../engine/skill-use.md#5-levels-7052) |
| Menu registry, menu stack, path-addressed access | [UI System § Menu registry, stack and path access](../engine/ui.md#menu-registry-stack-and-path-access-m48-design-phase-0) (written in Phase 0) |
| Loading screens | [UI System § loading screens](../engine/ui.md#original-game-loading-screens-during-scene-transitions) |
| Race and sex selection | [UI System § Race menu](../engine/ui.md#race-menu-gui-page-002--003) |
| Scaleform AS2 / AS3 extensions | [UI System § Scaleform host methods and remaining `_global.gfx` stubs](../engine/ui.md#scaleform-host-methods-and-remaining-_globalgfx-stubs) |
| Fonts and text replacement | [UI System § Strings and fonts](../engine/ui.md#strings-and-fonts-the-design-depends-on) and [§ Font loading](../engine/ui.md#font-loading) |
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
M48.5). When this PRD was drafted (2026-11-01), both tracks loaded the vanilla HUD over the world. What they did
not yet do was *behave*: menus called the host and got nothing back, the
Skyrim and Fallout 4 meters stayed empty, one menu could be open at a time,
scripts that opened or watched menus saw either nothing or a different answer
depending on which script language asked (and no script ran at all while
a native page other than dialogue was open), and every player-facing page
except the HUD was an engine-native page drawn by the engine's own overlay. Phases 1, 2 and 4 have closed most of that (§9).

This PRD is for the player of each in-scope title, and through
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
fork Ruffle's VM (its one patch to Ruffle is the text pass Q7 allows), and
will not fabricate host answers it has no canonical state for.

---

## 2. Terminology

| Term | Meaning |
|---|---|
| **Menu** | One named player-facing screen of a game, under the name the game's own scripts use (`InventoryMenu`, `HUD Menu`, `Dialog`, menu code `1009`). The unit of this document's promise. |
| **Surface** | What renders a menu: one of the four classes in §3. |
| **Original movie** | A game's own `.swf` from its interface archive (or a mod's replacement), run in Ruffle under a host profile ([ui.md](../engine/ui.md#why-ruffle)). |
| **Original document** | A game's own MenuXml `menus\*.xml`, run by `byroredux-menuxml`. |
| **Native page** | An engine-drawn page of the player menu (the `GameMenuPage` variants, `crates/native-ui/src/game_menu.rs`) or the native HUD layer (`draw_hud`, `crates/native-ui/src/hud.rs`). |
| **Host method** | A call a movie makes into the engine: a Skyrim `GameDelegate` method or a Fallout 4 `BGSCodeObj` function, recognised through the profile's catalog ([ui.md](../engine/ui.md#scaleform-host-methods-and-remaining-_globalgfx-stubs)). |
| **Behaviour class** | The engine's answer to one host method: one of H1–H4 (§3.2). |
| **Path-addressed access** | Reading, writing or invoking an ActionScript member by its dot path from the host (`_root.HUDMovieBaseInstance…`), the operation Scaleform exposes to C++ as `GFx::Movie::GetVariable` / `SetVariable` / `Invoke` ([Autodesk: Game Communication Overview](https://help.autodesk.com/cloudhelp/ENU/Scaleform-Help/scaleform_help/game_communication.html); [Autodesk: ActionScript](https://help.autodesk.com/cloudhelp/ENU/Scaleform-Help/scaleform_help/game_communication/c_actionscript.html)). |
| **Adapter injection** | Inserting a ByroRedux-owned bytecode adapter into a movie at load so the host can reach movie state through Ruffle's supported ExternalInterface boundary. Shipped for both VMs; the path adapters are `crates/ui/src/avm1_path.rs` and `crates/ui/src/avm2_host/path_helpers.rs`. |
| **Menu registry** | The one engine record of which menus are open, in which order, with which flags: `MenuRegistry` (`crates/core/src/ecs/resources/menu_registry/`), built in Phase 1 (`GUI-REG`). |
| **Menu flags** | Per-menu behaviour bits (pauses the game, shows the cursor, allows saving, blurs the world) plus a depth. Sources for vanilla menus are listed per entry in the GUI-REG-004 table. For *custom* Fallout 4 menus, F4SE's `MenuData` struct defines bits (`MenuFlags` 0x01 PauseGame, 0x04 ShowCursor, 0x800 DoNotPreventGameSave, 0x400000 BlurBackground; `Depth` a separate field), which gives one vanilla menu's value (MessageBoxMenu 0x8018499) and notes that not all flags are known ([FO4 CK: MenuData Struct](https://falloutck.uesp.net/wiki/MenuData_Struct_-_UI)). |
| **Menu mode** | The state scripts observe while a menu is open: ObScript's `Begin MenuMode` / `Begin GameMode` blocks and `MenuMode` function ([GECK: MenuMode](https://geck.uesp.net/wiki/MenuMode), [GECK: GameMode](https://geck.uesp.net/wiki/GameMode), [CS: MenuMode](https://cs.uesp.net/wiki/MenuMode)), and Papyrus `Utility.IsInMenuMode`, which the CK defines by the game being paused, so that dialogue does not count ([CK: IsInMenuMode](https://ck.uesp.net/wiki/IsInMenuMode_-_Utility), [FO4 CK: IsInMenuMode](https://falloutck.uesp.net/wiki/IsInMenuMode_-_Utility)). |
| **Menu code** | The number ObScript names a menu by (`MenuMode 1002`). Per game: [CS: MenuTypes](https://cs.uesp.net/wiki/Template:MenuTypes) for Oblivion, [GECK: MenuTypes](https://geck.uesp.net/wiki/Template:MenuTypes) for FO3 / FNV, and xEdit's `wbMenuModeEnum` (`wbDefinitionsCommon.pas`), which lists Oblivion's and FO3 / FNV's codes in one table. |

---

## 3. Core model

### 3.0 Surfaces

Every menu of every in-scope game has exactly one surface at any time:

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

Phase 0 replaced the hand-written inventory (GUI-REG-006): every menu of each game, with its Q1 target surface, is in the checked-in inventories `docs/data/game-ui/menu-inventory-{oblivion,fo3,fnv,skyrim-se,fallout4}.tsv`. What is built is recorded in the [ROADMAP M48 row](../../ROADMAP.md#tier-7--deep-gameplay-systems), which holds every published number, and the registry, stack and path service are designed in [UI System § Menu registry, stack and path access](../engine/ui.md#menu-registry-stack-and-path-access-m48-design-phase-0). The state before the work, on 2026-11-03, is revision 9's §3.1.

What is not built yet, re-measured at HEAD on 2026-11-27:

| # | Item | At HEAD |
|---|---|---|
| 1 | Scaleform AS2 extensions | No `_global.gfxExtensions` / `scaleform.gfx.*` surface exists in `crates/ui/src` (GUI-GFX-001, -003, -004; Phase 3) |
| 2 | Host behaviour | Every catalog method carries its Phase 0 class, and calls are classed live (`ui.host`). Handlers are registered for four Skyrim dialogue methods (`SERVED_METHODS`, `byroredux/src/scaleform_dialogue.rs`) and the message box's `buttonPress` (`scaleform_messagebox.rs`), and the HUD driver installs response handlers (`scaleform_hud.rs`). A headless Skyrim SE run on 2026-11-27 saw the dialogue movie's count go from `H1 served 0` to `H1 served 1` on one click (`ui.host` counts calls and does not name the served ones; `buttonPress` was not exercised). Coverage per game is not complete or published (GUI-HOST-002, -005, -006, GUI-EVT-004; Phase 3) |
| 3 | FO3 / FNV `-s` strings | `&-sName;` entities are not resolved: `expand_entity` (`crates/menuxml/src/parse.rs`) returns a name with a capital letter as the bare text `-sName`, and a name with none becomes `0` (read from the code; GUI-FONT-005, Phase 3) |
| 4 | UI frame cost | `--bench-frames` output has one combined `ui_ms` (`byroredux/src/app_events.rs`), whose timer also covers video and loading-screen ticks. There is no per-menu figure and nothing is published (GUI-OPS-003, -004; Phase 3's exit) |
| 5 | Race menu | `Game.ShowRaceMenu()` opens the native race and sex page (`Effect::ShowRaceMenu`, GUI-PAGE-002, -003); Phase 5's exit, scenario 9 on Skyrim SE and FNV, is open (#6667) |
| 6 | Wave 2 | Fallout 4's AS3 extensions, and Fallout 3 and Oblivion beyond the menus already served (Phase 6, #6668); Fallout 4's menu open and close events already run (`crates/scripting/src/fragment/menu_events.rs`) |

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

- **GUI-NG-001.** Reopening R4: no second Flash VM, no change to Ruffle's
  AVM1 or AVM2, and no Scaleform GFx linkage
  ([R4](../archive/roadmap-history.md#tier-7--deep-gameplay-systems-deferred-until-tier-14-proves-out);
  proprietary middleware is parsed, never linked). The one patch to Ruffle is
  Q7's `$`-key translation in text fields
  ([patch notes](../../third_party/ruffle/PATCHES.md)); any other patch needs
  a new decision.
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
- **GUI-NG-009.** Starfield UI. Phase 0 found no `BGSCodeObj`-shaped contract
  (§9.1), so Q6 puts Starfield out of M48. Its data-model contract needs a
  PRD of its own.

---

## 5. Requirements by area

### 5.1 Menu registry and stack (`GUI-REG`)

- **GUI-REG-001.** The engine MUST hold one menu registry listing every
  open menu by its game-facing name, in stacking order, with its surface
  and flags.
- **GUI-REG-002.** The extender `UI.IsMenuOpen` (served through the SDK
  alias `ui.menu-is-open`; whether it is served is `LMC`'s disposition), ObScript
  `MenuMode` and every engine-side "is a menu open" check MUST read the
  registry, so that the same open menu answers `true` to all of them; the
  type-level questions (`MenuMode` 0–3) count only the menus that count
  toward menu mode (GUI-EVT-006), so an open HUD alone answers `false`.
  The name
  and code tables are per game and live in the translate step (the menu
  codes of §2, [GECK: MenuTypes](https://geck.uesp.net/wiki/Template:MenuTypes),
  [CS: MenuTypes](https://cs.uesp.net/wiki/Template:MenuTypes) and xEdit's
  `wbMenuModeEnum`, which gives Oblivion's barter menu code 1008 and
  FO3 / FNV's code 1053; the Skyrim menu-name list,
  [CK: UI Script](https://ck.uesp.net/wiki/UI_Script); the Fallout 4 list,
  [FO4 CK: Menu](https://falloutck.uesp.net/wiki/Menu)).
- **GUI-REG-003.** A HUD menu MUST stay in the registry, reported open,
  while a modal menu is open above it; whether it also stays *drawn* follows
  the modal's flags (GUI-REG-004). Phase 0 settled the reported half for
  Skyrim and Fallout 4 (§9.1). **Phase 0: verify** what the original draws in
  that state, settled by a capture of the original or a documented source (the
  public SKSE64 / F4SE definitions and the GECK / CS pages name no rule); no
  open issue owns this (§9.1).
- **GUI-REG-004.** Each menu's flags MUST come from a per-game table in which
  every entry carries either a cited source or the mark `undocumented` with
  the engine default it gets and why. Phase 0 sourced the Skyrim and Fallout 4
  `IMenu` bits for part of each game's menus and found no source for a pause,
  menu-mode or save flag on Oblivion, FO3 or FNV (§9.1; the
  `menu-flags-*.tsv` tables in [the data directory](../data/game-ui/README.md)).
  F4SE's custom-menu `MenuData` (§2) gives one vanilla value
  ([FO4 CK: MenuData Struct](https://falloutck.uesp.net/wiki/MenuData_Struct_-_UI)).
  **Phase 0: verify** the entries still `undocumented`, settled by a
  documented source for them; no open issue owns this (§9.1).
- **GUI-REG-005.** All open movies MUST render through the one process wgpu
  device, shared since #2733 (`shared_descriptors`,
  `crates/ui/src/player.rs`), so that stacking N menus does not create N GPU
  devices; `ui.menus` (GUI-OPS-001) reports the device count.
- **GUI-REG-006.** Phase 0 MUST check in a per-game menu inventory: every
  menu name from the sources in GUI-REG-002 with its target surface (per
  Q1), its flags source and its script entry points. It is data, not code,
  and replaces §3.1. The GECK `MenuTypes` template elides codes (its list
  carries `...` gaps from 1004 to 1080); xEdit's `wbMenuModeEnum` names
  the FO3 / FNV codes it elides and leaves the others unnamed on Fallout,
  and its entries credited to mods (JIP LN, lStewieAI) are `LMC`'s, not
  vanilla. The two name FO3 / FNV code 1004 differently; Phase 0 settled it
  as `HUDMainMenu`, and codes no source names are recorded as unknown (§9.1).
  The VATS menu (code 1056) is listed like any other; what it shows is the
  [VATS PRD](vats.md)'s.
- **GUI-REG-007.** Closing, replacing or failing to load a menu MUST return
  input to the next owner in the stack (or the world) in the same frame.
- **GUI-REG-008.** The UI viewport MUST follow swapchain resizes: after a
  resize every open movie and document re-renders at the new extent
  instead of stretching. Built in Phase 1: `SwfPlayer::resize` resizes a movie in place through Ruffle's `Player::set_viewport_dimensions` (`crates/ui/src/player.rs`).
- **GUI-REG-009.** At most one registry menu MUST own input at a time, and
  it MUST be the topmost menu whose flags claim input; `ui.menus`
  (GUI-OPS-001) names it.
- **GUI-REG-010.** Whether the simulation advances while menus are open
  MUST follow the PauseGame flag (GUI-REG-004) of the open registry menus,
  whatever their surface; `ui.menus` (GUI-OPS-001) MUST report the pause
  state. The VATS menu is the stated exception: it never pauses the
  world on its own account, and world time during VATS belongs to the
  [VATS PRD](vats.md) (`VATS-UI-007`, `VATS-FO4-002`), so its GUI-REG-004
  entry carries no pause and `ui.menus` reports no pause on its account.
- **GUI-REG-011.** Papyrus `Utility.IsInMenuMode` (Skyrim, Fallout 4) MUST
  answer `true` exactly while the registry holds a menu whose flags pause
  the game, so that an open dialogue menu answers `false`, as the CK and
  Fallout 4 CK define it (§2 *Menu mode*).
- **GUI-REG-012.** Whether a menu counts toward menu mode (GUI-EVT-006) MUST
  be an entry of the GUI-REG-004 table.
- **GUI-REG-013.** The VATS menu's table entry (code 1056 on FO3 / FNV,
  `VATSMenu` on Fallout 4) MUST record that it never pauses the world on its
  own account, citing `VATS-UI-007` in the [VATS PRD](vats.md), whatever
  pause bit a source gives it.

### 5.2 Host-method behaviour (`GUI-HOST`)

- **GUI-HOST-001.** Every catalog entry of every in-scope profile MUST
  carry one behaviour class in the catalog itself (H1–H3 as a field; H4 is "not in the catalog", §3.2, so it is a runtime diagnostic class a catalog entry cannot have, Phase 0)
  (`crates/ui/src/catalog.rs`), next to the existing `provenance` field.
- **GUI-HOST-002.** An H1 method MUST act through the same canonical API the
  native page for that menu uses (inventory, equipment, barter, crafting,
  dialogue, settings, save), so there is one validation path per action;
  the observable is one test per H1 action that drives the action once
  through the host method and once through the native page's control and
  asserts the same resulting ECS state. The native pages already follow
  this rule for settings ("one validation/application path",
  `draw_game_menu` doc in `crates/native-ui/src/game_menu.rs`).
- **GUI-HOST-003.** An H2 answer MUST name its reason in the catalog, and
  methods whose purpose is an online service MUST be H2 with the reason
  "anti-scope: hosted services" (GUI-NG-003). Phase 0's per-method review
  lists them (the `host-classes-*.tsv` tables in
  [the data directory](../data/game-ui/README.md)).
- **GUI-HOST-004.** An H3 or H4 call MUST produce one attributed diagnostic
  per (menu, method) naming the class, as `ui.host` reports them, and MUST NOT be answered with
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
  prefix the AVM2 adapter already uses (the callback constants in
  `crates/ui/src/avm2_host.rs`), and every one whose absence is functional
  MUST be listed by exact name in `RESERVED_ENGINE_CALLBACKS`
  (`crates/ui/src/host.rs`). The prefix alone does not stop a movie
  registering the same names (that constant's doc comment records a movie
  occupying a prefix-keyed band before the adapter ran, #3434); exact-name
  reservation is the guarantee the bridge keeps today.
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
  (the `UI` branch of `classify_static_call`,
  `crates/sdk/src/compatibility/source_alias.rs`);
  which functions are served, and behind which capability, is `LMC`'s
  disposition (its catalog row S4) under that design.

### 5.4 HUD (`GUI-HUD`)

- **GUI-HUD-001.** With `--hud` on Skyrim, the vanilla `hudmenu.swf` health,
  magicka and stamina meters MUST follow the player's composed actor values;
  `hud.values 1.0` then `0.35` MUST change the meter band's pixels and leave
  a control band unchanged.
- **GUI-HUD-002.** The Fallout 4 twin of GUI-HUD-001 MUST hold for the
  health and action-point meters.
- **GUI-HUD-003.** The vanilla meter feed paths MUST be the ones the
  original engine drives, listed in checked-in data beside the catalog (the
  `hud-feeds-*.tsv` tables in [the data directory](../data/game-ui/README.md)).
  Phase 0 read them with the repository's own SWF readers and the executables'
  string tables, because JPEXS FFDec was not installed; Fallout 4's
  "engine fed" rows are inferred (§9.1).
- **GUI-HUD-004.** When an original HUD drives the player's vitals, the
  native HUD's vitals bars MUST NOT draw a second copy. Built:
  `vitals_snapshot` (`byroredux/src/inventory/catalog.rs`) returns nothing
  while the original HUD is visible.
- **GUI-HUD-005.** The compass of each original HUD MUST follow the camera
  heading, as the MenuXml HUDs already do through `cropx`
  ([ui.md](../engine/ui.md#menuxml-legacy-ui-m484-oblivion--m485-fo3)).
  Their feed paths are part of GUI-HUD-003's verify.
- **GUI-HUD-006.** The FO3 / FNV HUD art SHOULD be tinted by the game's HUD
  colour instead of rendering art-native white. Phase 0 found the default,
  `[Interface] iSystemColorHUDMainRed` / `Green` / `Blue` = 26 / 255 / 128, in
  both games' shipped `Fallout_default.ini` (§9.1); at HEAD on 2026-11-27 no
  code applies it.

### 5.5 Scaleform extensions (`GUI-GFX`)

- **GUI-GFX-001.** The AVM1 profile MUST provide the Scaleform AS2 extension
  members vanilla movies read, enabled the way Scaleform enables them
  (`_global.gfxExtensions = true`,
  [Autodesk: AS2 Extensions Reference](https://help.autodesk.com/cloudhelp/ENU/Scaleform-Help/scaleform_help/as2_reference.html)).
  None exist yet (§3.1 row 1).
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
  Skyrim's file is `interface\fontconfig.txt` (format in §9.1).
- **GUI-FONT-002.** The font library MUST be the one the active language
  installs: Skyrim ships one `fonts_en.swf` name for every language, with
  the encoding decided by the file ([UESP: String Table File Format § String
  Encodings](https://en.uesp.net/wiki/Skyrim_Mod:String_Table_File_Format)).
- **GUI-FONT-003.** A font class or glyph that does not resolve MUST fall
  back to the font Q5 chooses, with one diagnostic per (menu, class).
- **GUI-FONT-004.** Scaleform `$`-token text replacement MUST resolve from
  the game's vanilla translation tables (Skyrim and Fallout 4 paths and
  format in §9.1); mod tables are `LMC-UI-004`.
- **GUI-FONT-005.** FO3 / FNV MenuXml `-s`-prefixed strings (`&-sName;`
  entities) SHOULD resolve through a per-game table of the built-in game
  settings, with the plugin's `GMST` records overriding it and a
  case-insensitive lookup. **Settled in Phase 0 (§9.1):** they are
  game-setting references, but the plugin's `GMST` records alone resolve one
  name per game (1 of 102 on FO3, 1 of 145 on FNV); the defaults live in the
  executable. **Phase 0: verify** the FNV default text, settled by a readable
  source for it (its executable is packed; #6665).

### 5.7 Script and engine events (`GUI-EVT`)

- **GUI-EVT-001.** Opening or closing a menu in the registry MUST raise
  Fallout 4's `OnMenuOpenCloseEvent(asMenuName, abOpening)` on every script
  registered through `RegisterForMenuOpenCloseEvent` for that menu name,
  vanilla Papyrus on Fallout 4
  ([FO4 CK: OnMenuOpenCloseEvent](https://falloutck.uesp.net/wiki/OnMenuOpenCloseEvent_-_ScriptObject)).
- **GUI-EVT-002.** The registration MUST unregister automatically when the
  owning quest stops or the active magic effect is removed, as the CK page
  documents.
- **GUI-EVT-003.** Script functions that open a menu MUST open it through
  the registry, under the game-facing name the per-game inventory gives, with
  the surface the inventory names, or report S4 with the fallback the effect
  already uses (a placeholder effect, `Effect::is_placeholder`, or the
  `script.requests` log for the requests nothing opens). The Phase 0 inventory
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
  the registry is presentation state, republished every frame, which `MenuRegistry` carries as a not-saved-by-design entry (`byroredux/src/save_io/registry_completeness_tests.rs`).
- **GUI-EVT-006.** ObScript `Begin MenuMode [code]` blocks MUST run on the
  frames the registry holds a menu matching the block's code (any
  menu-mode menu when it names none), and `Begin GameMode` blocks MUST NOT
  run on those frames, as the GECK and CS block pages define them (§2
  *Menu mode*), even while GUI-REG-010 has the simulation paused. Which
  registry menus count toward menu mode is a per-menu entry of the
  GUI-REG-004 table; a HUD menu does not, since GameMode blocks run "during
  normal gameplay" while the HUD is up ([GECK: GameMode](https://geck.uesp.net/wiki/GameMode)).
  `Begin MenuMode` compiles as block type `0x0001` on all three ObScript
  games, and its optional menu code as the block's one numeric argument
  (§9.1; `BLOCK_MENU_MODE`, `crates/scripting/src/obscript_vm.rs`).

### 5.8 Native pages and residues (`GUI-PAGE`)

- **GUI-PAGE-001.** Every native page MUST register in the registry under
  the game-facing name of the menu it stands in for (`InventoryMenu`,
  `Dialog`, code `1009`, …), so that scripts see it as that menu.
- **GUI-PAGE-002.** `Game.ShowRaceMenu()` / ObScript `ShowRaceMenu` MUST open
  an interactive race and sex selection on the surface Q4 chooses, listed
  open in the registry until the player accepts. The vanilla quest advances
  differently per caller (§9.1,
  [caller table](../data/game-ui/race-menu-caller-wait.tsv)). **Phase 0:
  verify** whether the original call returns before its menu closes (#6667),
  settled by observing the original or an external source: the vanilla callers
  read so far either work if it does or say nothing, and none tells a
  blocking call from a non-blocking one. The CK and GECK pages
  ([CK: ShowRaceMenu](https://ck.uesp.net/wiki/ShowRaceMenu_-_Game),
  [GECK: ShowRaceMenu](https://geck.uesp.net/wiki/ShowRaceMenu)) do not
  say. The CS page ([CS: ShowRaceMenu](https://cs.uesp.net/wiki/ShowRaceMenu))
  warns that calling more than one character-creation menu at a time
  freezes controls and crashes and that scripts must pause between the
  calls, which is consistent with Oblivion's call returning while its menu
  is still open.
- **GUI-PAGE-007.** Until GUI-PAGE-002's verify settles otherwise, the
  engine's `ShowRaceMenu` MUST NOT hold the calling script (an engine
  choice), so that menu-mode blocks run while the menu is open
  (GUI-EVT-006) and GameMode blocks resume after it closes.
- **GUI-PAGE-003.** The Fallout 4 `ShowRaceMenu` parameters (`uiMode` 0–4,
  the menu target and vendor, [FO4 CK: ShowRaceMenu](https://falloutck.uesp.net/wiki/ShowRaceMenu_-_Game))
  MUST lower.
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
  operator overlay crate. They do: `crates/native-ui` holds them, apart from the F3 operator panels in `byroredux-debug-ui`.

### 5.9 MenuXml menus beyond the HUD (`GUI-XML`)

- **GUI-XML-001.** The MenuXml track MUST load any menu document of the
  three pre-Skyrim games through the same `MenuProfile`, not only
  `hud_main_menu.xml`; which menus become S2 follows Q1.
- **GUI-XML-002.** MenuXml input (pointer hit-testing over tile rects,
  keyboard navigation) MUST route through the same `UiInputEvent` contract
  the Scaleform track uses (`crates/ui/src/input.rs`), so that one input
  owner rule (GUI-REG-009) covers both tracks.
- **GUI-XML-003.** Values a MenuXml menu reads from the engine MUST be set
  through the override API the HUD uses (`MenuRenderer::set_override`,
  `instantiate_template`, `graft_fragment` in `crates/menuxml/src/menu.rs`),
  so that the per-game difference stays in the profile.
- **GUI-XML-004.** Oblivion's sleep/rest and level-up driver MUST serve
  `SleepWaitMenu` (1012, `menus\sleep_wait_menu.xml`) and `LevelUpMenu`
  (1027, `menus\levelup_menu.xml`) as S2 through Oblivion's `MenuProfile`.
  A native page MAY serve either menu as the S3 fallback under Q1 and
  MUST use the same registry name, code and gameplay contract. Both
  documents ship in `Oblivion - Misc.bsa` and are inventoried in
  [the Oblivion menu table](../data/game-ui/menu-inventory-oblivion.tsv).
  The driver is `byroredux/src/menuxml_rest.rs` (#7086).
- **GUI-XML-005.** Activating a bed or bedroll the Oblivion player may use
  MUST open `SleepWaitMenu` in sleep mode. `bed_use_system` handles the
  ordinary bed activation.
- **GUI-XML-006.** A confirmed bed sleep that reaches its requested end
  MUST emit exactly one `SleepCompleted` with the elapsed hours and
  `interrupted = false` to the existing level-up consumer (OSL-SLP-001).
- **GUI-XML-007.** When that uninterrupted sleep ends with an eligible
  pending level, `LevelUpMenu` MUST open, showing the next level and the
  oldest pending level's attribute choices and earned bonuses.
- **GUI-XML-008.** `SleepWaitMenu` MUST show the chosen duration and let the
  player confirm or cancel.
- **GUI-XML-009.** A wait, which `T` requests (the engine's binding), MUST open
  `SleepWaitMenu` showing its wait prompt, not its sleep prompt:
  `menus\sleep_wait_menu.xml` holds `_waitquestion` and `_restquestion` and
  selects one through the root's `user0` (read from `Oblivion - Misc.bsa`
  with `bsa_extract_one` on 2026-11-27).
- **GUI-XML-010.** Which beds and bedrolls the player may use MUST follow the
  original's rules for
  [legal beds and bedrolls](https://en.uesp.net/wiki/Oblivion:Sleeping_and_Eating).
- **GUI-XML-011.** A sleep that an enemy encounter ends early MUST NOT emit a
  successful completion; it MAY emit `SleepCompleted` with
  `interrupted = true` (OSL-SLP-002). This places no requirement on encounter
  generation.
- **GUI-XML-012.** Waiting, and cancelling before confirmation, MUST emit no
  `SleepCompleted`.
- **GUI-XML-013.** The runner MUST recheck combat and bed availability before
  the time skip.
- **GUI-XML-014.** Each `LevelUpMenu` row MUST take its attribute identity,
  current base value, eligibility and frozen bonus from the leveling system
  (OSL-ATT-001).
- **GUI-XML-015.** The player MUST be able to choose and revise three distinct
  eligible attributes before confirming.
- **GUI-XML-016.** Confirmation MUST submit all three picks together to the
  shared level-up path for that sleep, applying exactly one level; the
  gameplay system validates eligibility and owns stat changes.
- **GUI-XML-017.** The console path MAY accept picks ahead of the next
  completed scripted sleep.

### 5.10 Observability and cost (`GUI-OPS`)

- **GUI-OPS-001.** A `ui.menus` console command MUST list every registry
  entry: name, surface, flags, input owner, and the open-movie device
  count and pause state. It joins `hud.status` / `hud.debug`, which stay.
- **GUI-OPS-002.** A `ui.host <menu>` console command MUST report, for one
  open menu, the count of calls per behaviour class and the distinct H3 /
  H4 method names, bounded the way the drain's diagnostic set is bounded
  today (`MAX_DISTINCT_HOST_METHOD_NAMES`, defined in `crates/ui/src/host.rs`,
  applied in the host-report drain in `byroredux/src/app_frame.rs`).
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
  menus headless; the next `ui.menus` line MUST show the change.

---

## 6. Deliberate deviations and risks

| # | Original behaviour | Redux behaviour | Becomes redundant | Risk and its requirement |
|---|---|---|---|---|
| 1 | Native code can drive menus by path through GFx `Invoke` / `SetVariable` ([Autodesk](https://help.autodesk.com/cloudhelp/ENU/Scaleform-Help/scaleform_help/game_communication.html)), the operation SKSE's `UI.Set*` / `Invoke*` expose by menu name and `_root` path ([CK: UI Script](https://ck.uesp.net/wiki/UI_Script)); which paths the vanilla HUDs are fed through is GUI-HUD-003's verify | A bytecode adapter injected at load serves path access through ExternalInterface (GUI-PATH-001) | The SkyUI-class poll handlers as the only meter feed (they stay, behind the general table, GUI-HOST-005) | A movie that rebuilds its root after load loses the adapter → GUI-PATH-002 reports the absent path; GUI-PATH-004 keeps names apart |
| 2 | Every menu is the game's own movie or document | Some menus are native pages (Q1) | Nothing | Mod movies for those menus do not run → GUI-PAGE-001 reports the surface, `LMC-UI-001` reports the mod as affected |
| 3 | `ShowRaceMenu` opens a menu that also edits the face and hair ([GECK: ShowRaceMenu](https://geck.uesp.net/wiki/ShowRaceMenu); Fallout 4's modes include haircut, plastic surgery and face paint, [FO4 CK: ShowRaceMenu](https://falloutck.uesp.net/wiki/ShowRaceMenu_-_Game)) | Race/sex selection only (GUI-PAGE-002, Q4) | Nothing further: `ShowRaceMenu` already opens the native page and is no longer a placeholder effect | Appearance cannot be edited → GUI-PAGE-003 reports the other modes S4 |
| 4 | Online and storefront menus reach a hosted store ([UESP: Creation Club](https://en.uesp.net/wiki/Skyrim:Creation_Club)) | H2 placeholder, anti-scope (GUI-HOST-003) | Nothing | A menu waits on an answer that never comes → H2 answers immediately with the offline value |
| 5 | Lockpicking / hacking open a minigame menu (GECK *MenuTypes* 1014 LockPick and 1055 Hacking; `LockpickingMenu` in [FO4 CK: Menu](https://falloutck.uesp.net/wiki/Menu)) | No menu; engine policy (`lockpick.rs`) | Nothing | `OnMenuOpenCloseEvent("LockpickingMenu")` never fires → GUI-NG-004 reports the menu S4 |
| 6 | Skyrim ships one `fonts_en.swf` name for every language, the encoding decided by the file ([UESP: String Table File Format § String Encodings](https://en.uesp.net/wiki/Skyrim_Mod:String_Table_File_Format)) | Missing class or glyph falls back (GUI-FONT-003, Q5) | Nothing | Visible font mismatch → the diagnostic names the class |
| 7 | **Phase 0: verify** whether the original menus share one GFx runtime, settled by the SKSE64 and F4SE Scaleform loader definitions | Movies share one wgpu device (GUI-REG-005) | Nothing: already built | None known; `ui.menus` reports the device count |
| 8 | Each menu's flags are the game's own | Values no source documents get a stated engine default (GUI-REG-004) | Nothing | A menu pauses, or allows saving, where the original did not → the flags table names the defaulted entries |
| 9 | **Phase 0: verify** that Scaleform itself shows a translation for a text field holding a `$` key, settled by Autodesk's translator documentation or the SKSE64 translation source | Ruffle has no such pass, so three vendored crates (`ruffle_core`, `ruffle_common`, `ruffle_macros`) carry one text-field patch (Q7, [patch notes](../../third_party/ruffle/PATCHES.md)); against upstream at the pinned revision on 2026-11-27, `core` differs in one new module, its declaration and three call sites | Nothing | Each Ruffle revision bump must re-vendor the crates and re-apply the patch → the patch notes give the steps; its test is `the_ruffle_text_pass_translates_installed_keys` (GUI-FONT-004) |
| 10 | A flags source may give the VATS menu a pause bit | The VATS entry never pauses on its own account (GUI-REG-010, -013) | Nothing | The engine ignores a sourced bit → GUI-REG-013 records it; world time during VATS is `VATS-UI-007`'s |
| 11 | **Phase 0: verify** what the original draws for a HUD under a modal (GUI-REG-003) | Until measured, the registry keeps the HUD open and nothing hides it | Nothing | The HUD may draw over a menu where the original hid it → GUI-REG-003 |

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
| Oblivion | yes | Wave 2, ahead of its engine Tier 3 because it shares FNV's MenuXml machinery and its HUD already ships (M48.4). Sleep/rest and level-up use the Oblivion MenuXml driver (GUI-XML-004…017). |
| Fallout 3 | yes | Wave 2. MenuXml track, HUD shipped (M48.5); shares FNV's profile, so its MenuXml menus (GUI-XML) land with FNV's in Phase 3, which the [VATS PRD](vats.md) depends on |
| Fallout: New Vegas | yes | Wave 1. Reference title and second playable route; MenuXml track |
| Skyrim (LE / SE / AE) | yes | Wave 1. First playable route; AVM1 profile and the SkyUI-class protocols, measured on SE. LE is parse-only in the [compatibility matrix](../../ROADMAP.md#compatibility-matrix), so its live scenarios wait for LE cells to load. LE needs no new profile (§9.1) but has two gaps, both #6826's: `ToggleFavorite` is not in the catalog, and LE ships some menus only as `interface\exported\*.gfx` while the HUD path is fixed to `interface\hudmenu.swf` |
| Fallout 4 | yes | Wave 2. AVM2 profile and adapter shipped (M48.7) |
| Fallout 76 | no | Tier 3, parse only: no cell loads, so no HUD or menu has a world to sit over ([game-compatibility.md](../engine/game-compatibility.md#fallout-76)); revisit when FO76 cells load |
| Starfield | no | Out of M48 by Q6: Phase 0 found no `BGSCodeObj`-shaped contract (§9.1, GUI-NG-009). A profile for its data-model contract needs its own PRD; the rest of #4470's bring-up has no tracking issue |

No requirement adds a per-game branch outside the host profiles, the
`MenuProfile` / `HudGameProfile` tables and the per-game menu inventory;
those are this layer's translate step (GUI-HOST-006, GUI-XML-003).

### 7.2 Acceptance

Data: installed vanilla archives only (`Skyrim - Interface.bsa`,
`Fallout4 - Interface.ba2`, `Fallout - Misc.bsa`, `Oblivion - Misc.bsa`,
their texture archives); nothing proprietary is checked in, the rule the
ignored corpus tests already follow ([ui.md](../engine/ui.md#tests)).

A scenario passes when it holds as stated below. Issues cite scenario *N* as
A*N*.

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
8. **Menu events.** New script `m48-menu-events.sh` (Fallout 4): the
   vanilla script `vaultexitelevatorscript`, registered by
   `RegisterForMenuOpenCloseEvent`, receives one opening and one closing
   event (GUI-EVT-001). Its handler is reached only after its trigger opens
   `SPECIALMenu` / `LooksMenu`, so it waits on those routing through the
   registry (§9.1).
9. **Race menu.** New script `m48-race-menu.sh`.
   Skyrim SE: set MQ101 to stage 75, whose `Fragment_13` calls
   `Game.ShowRaceMenu()` with no condition (what sets that stage is unknown;
   the two stage 0 callers are developer quickstarts, #6824). `ui.menus`
   lists `RaceSex Menu` open until `ui.close "RaceSex Menu" accept` closes
   it. FNV: set `VCG01` to stage 36, whose result script calls
   `ShowRaceMenu`; `ui.menus` lists `RaceSexMenu` open, the quest reads stage
   40 while it is still open (its `Begin MenuMode 1036` block), and
   `ui.close RaceSexMenu accept` closes it (GUI-PAGE-002, -007,
   GUI-OPS-005).
10. **Menu mode and pause.** New `cargo test -p byroredux --bin byroredux`
    test `menu_mode_blocks_and_pause_follow_the_registry`, on registry
    entries and a synthetic ObScript object script with one `Begin
    MenuMode 1002` and one `Begin GameMode` block, on the FNV tables: with
    an Inventory entry whose flags pause, the simulation is paused, the
    MenuMode block runs and the GameMode block does not; with only the HUD
    entry, the simulation advances, the GameMode block runs and the
    MenuMode block does not (GUI-REG-010, GUI-EVT-006).
11. **Oblivion bed sleep and level-up.** Load `Oblivion.esm` and
    `Oblivion - Misc.bsa` in `WeynonPrioryHouse`, with a usable upstairs
    bed ([UESP: Weynon House](https://en.uesp.net/wiki/Oblivion:Weynon_House)).
    Bank two pending levels with different frozen bonuses and activate the bed
    through gameplay. Check `ui.menus` for code 1012 on the MenuXml
    document. Choose and confirm one hour. Check for one hour of game
    time, one successful sleep completion and code 1027 on its MenuXml
    document. Check that the level is unchanged while the player edits
    their three picks. Confirm and assert that only those attributes
    rise by their earned bonuses up to 100, the level rises by one, and
    only the oldest pending level is consumed (GUI-XML-005…008, -014…-016).
12. **Oblivion sleep refusals and fallback.** Reset the pending-level
    fixture for each leg. Cancel the duration picker, confirm a wait, and
    have a deterministic test interruption end a confirmed sleep before
    its requested duration. Each yields zero successful sleep completions,
    leaves the pending level and attributes unchanged, and keeps
    `LevelUpMenu` closed. An uninterrupted sleep
    with no pending level produces a completion and returns to play.
    Repeat the successful menu actions with the corresponding MenuXml
    document withheld and the native fallback installed. Check the same
    menu codes and leveling results (GUI-XML-009, -011…-013).

For scenarios 11 and 12, the headless live script is **not written**. Engine
tests cover part of them: `oblivion_bed_sleep_offers_only_the_oldest_pending_level`
and `oblivion_rest_cancel_wait_and_interrupt_keep_a_pending_level`
(`byroredux/src/skill_use_emit.rs`; passed at HEAD on 2026-11-27),
`installed_oblivion_rest_documents_display_and_route_their_actions`
(`menuxml_rest.rs`, ignored without installed data; not run on that date) and
`oblivion_rest_fallback_draws_listed_labels_and_attribute_data`
(`crates/native-ui/src/page_strings_tests.rs`; not run on that date).

---

## 8. Metrics

One home for the numbers: the ROADMAP M48 row and, per game, the
[compatibility matrix](../../ROADMAP.md#compatibility-matrix). The Legacy
Mod Compatibility PRD's *UI method coverage* counts methods called by
*mod* movies; the coverage below counts vanilla movies only, so the two
never share a denominator.

| Metric | Definition | Granularity | ROADMAP home |
|---|---|---|---|
| Menu surface coverage | menus with surface S1–S3 ÷ menus in the game's registry table, as `ui.coverage` counts them. The table is built in `crates/core/src/ecs/resources/menu_registry/tables.rs` from the flags and code tables joined to the inventory's `top-level menu` and `hud` rows; it is not the inventory's row count | per game, per surface class | compatibility matrix, per game |
| Host behaviour coverage | H1 methods ÷ catalog methods the vanilla interface archive calls | per game | M48 row |
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

### 9.1 Phase 0 results (2026-11-08, #6643; corrected 2026-11-27)

Answers measured on this machine's installs. Each row names its tool or data file so it can be
re-run; data tables are in [`docs/data/game-ui/`](../data/game-ui/README.md), which holds the counts, and the design is the
[ui.md section](../engine/ui.md#menu-registry-stack-and-path-access-m48-design-phase-0). Rows marked *corrected* replace a
statement that revision 10 carried and that the 2026-11-27 re-check refuted.

| Item | Answer | Source |
|---|---|---|
| GUI-EVT-006 block type | `Begin MenuMode` is compiled block type `0x0001` on Oblivion, FO3 and FNV. `0x0009` is `OnHitWith`. | `obscript_tools census <ESM>` "block names" |
| GUI-EVT-006 menu codes (*corrected*) | Vanilla **does** use numeric codes. Source lines opening a block, `Begin MenuMode <code>`: Oblivion 10, FO3 3, FNV 36 (for example `VCG01SCRIPT` `00104C00`, code 1036). `MenuMode <code>` function calls: FNV 2 (`Lucky38VictorElevatorScript` `0015725D`, `vMojaveExpressBoxSCRIPT` `0014B062`), FO3 and Oblivion none. | A 2026-11-27 line search of every `SCTX` in `SCPT`, `QUST`, `INFO`, `PACK`, `TERM`, `PERK` and `IDLM` records (not checked in); the named records can be read in xEdit |
| GUI-HUD-006 | `[Interface] iSystemColorHUDMainRed` / `Green` / `Blue` = **26 / 255 / 128** in the shipped `Fallout_default.ini` of FO3 and FNV. Siblings: `HUDAlt` 255 / 67 / 42, `Pipboy` and `System` 26 / 255 / 128, `Terminal` 33 / 231 / 121, `MainMenu` 199 / 255 / 165. | `Fallout_default.ini` of both installs |
| GUI-FONT-001 | Skyrim: `interface\fontconfig.txt` in `Skyrim - Interface.bsa`, with `_cn`, `_ja`, `_pl`, `_ru` variants. CRLF text: `fontlib "<swf>"` lines, then `map "$ConsoleFont" = "Consolas" Normal` lines (weight `Normal` / `Bold`). Fallout 4 ships the same name with `_cn`, `_ja`. | archive listing and extraction |
| GUI-FONT-004 | Skyrim `interface\translate_<language>.txt` (language spelled out); Fallout 4 the same with short tags. UTF-16 LE with BOM, CRLF, `$KEY<TAB>value`. FO3 and FNV ship no translation or font file in `Fallout - Misc.bsa`. | the interface archive listings |
| Scenario 8 fixture | `vaultexitelevatorscript`. The one small alternative, `audiocontainernoanimscript`, calls `Sound.PlayAndWait`, which the VM does not model. | `r5_extract_pex_ba2` and a string search of `Fallout4 - Misc.ba2` |
| `ShowRaceMenu` callers (*corrected*) | **FNV:** quest `VCG01` (`00104C1C`) stage 36, the opening sequence, and `VCG04ActivatorScript` (`001164A4`). **FO3:** quest `CG00` (`0001F388`) stage 60 and `Vault101ExitScript` (`0005CF93`). **Oblivion:** `CGSewerExitScript` (`0000A396`). **Skyrim:** `QF_MQ101_0003372B` `Fragment_156` and `_27` (stage 0, gated by `MQQuickstart`) and `_13` (stage 75), and `TriggerRaceMenuScript::OnLoad`. **Fallout 4** includes `SF_MQ101_001_MirrorScene_000A7D3C`. | [caller table](../data/game-ui/race-menu-caller-wait.tsv); the quest-stage calls come from the `SCTX` search above (`obscript_tools dump` reads `SCPT` only) |
| GUI-PAGE-002 race menu wait (*corrected*) | No script found here **proves** whether the call returns before the menu closes. FNV `VCG01`: the call is the stage's only command, and `Begin MenuMode 1036` sets stage 40 while the menu is open. FO3 `CG00`: the call is followed by a timer flag; a `Begin MenuMode` block sets stage 61 while the menu is open, and a GameMode timer sets 62 after it. The other Fallout, Oblivion, Skyrim and Fallout 4 callers read by Phase 0 are written to work if it returns at once, or say nothing; Skyrim's evidence is indirect. | [caller table](../data/game-ui/race-menu-caller-wait.tsv) |
| GUI-REG-006 inventory | Checked in per game, with the Q1 target surface derived from the host classes: MenuXml documents take S2; a movie whose every host call is H1 or H2 takes S1; one H3 call sends the menu to S3 or S4 `recorded`. Skyrim and Fallout 4 menu-to-movie joins are by file name only. | `menu-inventory-*.tsv` |
| GUI-REG-006 code 1004 | **`HUDMainMenu`.** xEdit's `MainMenu` is the title screen, which has no code of its own. 1026 is vanilla (`BookMenu`) although xEdit credits it to a mod. Unknown codes: FO3 / FNV 1005, 1006, 1010, 1011, 1015, 1017-1022, 1024, 1025, 1028-1034, 1037-1046, 1049, 1050, 1052, 1062-1073, 1078, 1079 (and Audio / Video / Gameplay); Oblivion 1031, 1033, 1050 unknown whether vanilla. | `menu-codes-fnv-fo3.tsv`, `menu-codes-oblivion.tsv`; xNVSE, xOBSE and xEdit sources pinned there |
| GUI-REG-004 flags | Skyrim and Fallout 4 share the `IMenu` bit set; "counts toward menu mode" equals `kPausesGame`, and neither HUD carries it. Oblivion, FO3 and FNV document no pause, menu-mode or save flag; the one per-menu datum is the XML `<stackingtype>`. Skyrim LE has no source. | `menu-flags-*.tsv` |
| GUI-REG-003 HUD under a modal | **Reported open: settled** for Skyrim and Fallout 4 (`IsMenuOpen` is the on-stack bit; neither HUD is modal or pausing). **Drawn: not settled.** | `menu-flags-skyrim.tsv`, `menu-flags-fallout4.tsv`; CommonLibSSE `UI.h`, commonlibf4 `GameMenuBase.h` |
| GUI-HOST-001 / -003 classes | Every catalog entry has a target class; no entry can be H4 (§3.2), and Skyrim's `GameDelegate.call` sites with run-time names are the measured H4 risk. Online-service methods are H2. A handler must return a value wherever a movie uses a `Command`'s result. | `host-classes-*.tsv` |
| GUI-GFX-003 extension members | Measured per game. Every Fallout 4 and Starfield movie using `scaleform.gfx.*` defines those classes itself as no-op stubs, so GUI-GFX-002 means overriding them by ABC injection. The pinned Ruffle has none of the AS2 extension names. | `gfx-extensions-*.tsv`; `crates/ui/examples/gfx_extension_census.rs` |
| GUI-HUD-003 / -005 feed paths | Skyrim feeds sit under `_root.HUDMovieBaseInstance`; Fallout 4's are document-relative with 0..1 setters, and `HUDCompassWidget` has no ActionScript (later served by #7036). FFDec was not installed; Fallout 4 "engine fed" rows are inferred. | `hud-feeds-*.tsv`; `crates/ui/examples/hud_feed_paths.rs` |
| GUI-FONT-005 | `&-sName;` XML entities name built-in settings whose default text is in the executable; the masters' `GMST` records resolve almost none. FO3 defaults are readable; FNV's executable is SteamStub-packed. Lookup is case-insensitive. | `menu-string-refs-*.tsv`; `crates/plugin/examples/menu_gmst_census.rs strings` |
| GUI-PAGE-005 strings | Engine text and vanilla equivalents are listed per page; the residues Phase 0 found are fixed (#6825). | `native-page-strings.tsv` |
| §7.1 Skyrim LE | **No new profile.** Two gaps: `ToggleFavorite` is missing from the catalog, and LE ships some menus only as `interface\exported\*.gfx` while `ScaleformGame::menu_path` is fixed to `interface\hudmenu.swf`. | `host-diff-skyrim-le-vs-se.tsv` |
| §7.1 Starfield / Q6 | **Out of M48.** Its movies, the HUD included, use a `BSUIDataManager` / `UIDataShuttleConnector` data-model contract, not a `BGSCodeObj` function table, so the Fallout 4 adapter finds nothing. | `gfx-extensions-starfield.tsv` |
| Q2 first question | **Yes:** an AVM1 adapter reaches `_root` members through ExternalInterface at the pinned Ruffle. | `crates/ui/tests/avm1_path_access_probe.rs` |
| GUI-REG-005 / -008 | The wgpu device was already shared per process (#2733); a viewport resize is in place at the pinned Ruffle. | [ui.md § Menu registry, stack and path access](../engine/ui.md#menu-registry-stack-and-path-access-m48-design-phase-0) |
| Q5 font | **Noto Sans (SIL OFL 1.1), Regular and Bold**, bundled with its licence; CJK stays a per-language fallback. | [ui.md § Strings and fonts](../engine/ui.md#strings-and-fonts-the-design-depends-on) |

`crates/plugin/examples/info_result_census.rs` greps the `INFO` result scripts of an ObScript-era master.

**Open at the Phase 0 exit**, with each item's state re-checked on 2026-11-27. The table lists what is still
open; the other items it named at the exit are answered (the Fallout 4 compass, #7036; `ShowMap`, #6823; the
native-page residues, #6825; the AVM2 path adapter, which Phase 2 built with its absent-path test).

| Item | Why open | Owner |
|---|---|---|
| GUI-REG-003, HUD *drawn* under a modal | No public source names the rule. The registry default (HUD keeps drawing) stands until measured. | **No open issue** (the Phase 1 owner, #6653, is closed) |
| Oblivion / FO3 / FNV menu flags and unnamed menu codes (GUI-REG-004 / -006) | Undocumented in every source read; the engine default stands. Skyrim and Fallout 4 menus with no sourced flags are in the same state. | **No open issue** (#6653 is closed) |
| Skyrim `call` callback reaching the vitals and compass angle | Bytecode reading only. The HUD meters are fed through the path service, so nothing depends on it today. | #6826 |
| Ruffle's behaviour for the `provisional:` extension rows | Rest on Autodesk semantics, not a Ruffle test. | #6826 |
| Skyrim LE: `ToggleFavorite`, exported `.gfx` menu paths | Catalog and path-resolution gaps; LE is in scope (§7.1). | #6826 |
| FNV default setting text (GUI-FONT-005) | Packed executable. | #6665 |
| Does the runtime honour the `MQQuickstart` stage 0 conditions | Unchecked. | #6824 |
| Whether `ShowRaceMenu` returns before its menu closes, on every game (GUI-PAGE-002) | No script found tells the two apart. | #6667 |

**Phase 1 — Registry and stack.** GUI-REG-001…011, GUI-PAGE-001,
GUI-EVT-005/006, GUI-OPS-001, GUI-OPS-005. Exit: scenarios 2, 3 and 10
pass, and scenario 1's floor passes.
**Closed** (#6653); results in the ROADMAP M48 row.

**Phase 2 — Path service and live HUD meters.** GUI-PATH-001…005,
GUI-HUD-001…005. Exit: scenarios 4, 5 and 6 pass; HUD meter liveness is
published for Skyrim and Fallout 4.
**Closed** (#6661); results and HUD meter liveness in the ROADMAP M48 row.

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
**Partly built** (#6665): host classes and `ui.host`, Skyrim fonts through `fontconfig.txt`, script-opened menus reported as S4, and FO3 / FNV MenuXml input are in the ROADMAP M48 row. **Open at HEAD (2026-11-27):** §3.1 rows 1 to 4.

**Phase 4 — Surfaces per Q1, wave 1.** The dialogue menu first
(GUI-PAGE-004), then the menus Q1 assigns, on Skyrim and FNV;
GUI-PAGE-005/006. Exit: Phase P passes on the chosen surface on both
games (`dt1-dialogue-layers.sh` on FNV, the new Phase P leg of
`dt2-skyrim-forcegreet.sh` on Skyrim, GUI-PAGE-004), and menu surface
coverage is published for both.
**Closed** (#6666); Skyrim dialogue runs on `dialoguemenu.swf` (S1) and New Vegas dialogue on `dialog_menu.xml` (S2), with the results in the ROADMAP M48 row.

**Phase 5 — Race menu.** GUI-PAGE-002/003/007 per Q4. Exit: scenario 9 passes
on Skyrim SE and FNV, and the MQ101 Known Issue is closed in ROADMAP.
**Open** (#6667): §3.1 row 5.

**Phase 6 — Wave 2.** Fallout 4 (GUI-GFX-002, GUI-EVT-001/002), Fallout 3,
Oblivion. Exit: scenario 8 passes, and every metric of §8 is published for
every in-scope game.
**Open** (#6668): §3.1 row 6.

**Phase 7 — Oblivion sleep and level-up menus** (#7048, built in #7086).
GUI-XML-004…017, on the playable bindings of [Oblivion Skill-Use
Leveling Phase 3](oblivion-skill-use-leveling.md#9-delivery-sequence)
(#7047). Exit: scenarios 11 and 12 pass.
**Closed as issues** (#7086) on a live Weynon House run, with both the
installed MenuXml documents and the native fallback, and on engine tests. The
scripted headless check for the two scenarios is not written (§7.2), so the
exit has no re-runnable measurement yet.

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
   **Decided** (2026-11-03): the recommendation — the game's own movie or
   document (S1 / S2) for every menu whose host calls are all served (H1),
   the native page (S3) as the fallback for missing data, unserved menus
   and Starfield, and the dialogue menu first.
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
   **Decided** (2026-11-03): the recommendation — adapter injection on both
   VMs, an AVM1 adapter mirroring the shipped AVM2 one, serving
   path-addressed access (GUI-PATH); scripts get no raw path access.
3. **Q3 — One movie per open menu or one modal movie.** Recommended: one
   Ruffle player per open menu on a shared device (GUI-REG-005), every
   visible layer rendered, only the top input owner ticking input. It is the
   only model in which a message box draws over an inventory and the HUD
   stays live. Alternative: at most one modal movie plus the HUD.
   Consequence: cheaper, but a menu that opens another (a message box from
   inventory) replaces it, which changes what scripts observe.
   **Decided** (2026-11-03): the recommendation — one Ruffle player per
   open menu on a shared device (GUI-REG-005), every visible layer
   rendered, only the top input owner ticking input.
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
   **Decided** (2026-11-03): the recommendation — an interactive race and
   sex selection (plus name where the game's menu has it), open in the
   registry until accepted; whether the caller waits follows GUI-PAGE-002's
   Phase 0 check per game.
5. **Q5 — Font fallback.** Recommended: a bundled, redistributable
   open-licence font family chosen in Phase 0, so captures are deterministic
   and pixel gates stay stable on every Linux install. Alternative: the
   system font configuration. Consequence: no bundled file, but glyph
   metrics vary by distribution, and a capture-based gate can pass on one
   machine and fail on another.
   **Decided** (2026-11-03): the recommendation — a bundled,
   redistributable open-licence font family chosen in Phase 0.
   **Phase 0 result** (2026-11-08): Noto Sans (SIL OFL 1.1), Regular and Bold;
   CJK stays a per-language fallback (§9.1).
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
   **Decided** (2026-11-03): the recommendation — Starfield is Wave 3
   inside M48 if the Phase 0 scan finds a `BGSCodeObj`-shaped contract the
   AVM2 profile can extend; otherwise out of M48 with a reason.
   **Phase 0 result** (2026-11-08): the scan found no such contract, so
   Starfield is **out of M48**, with the reason in §9.1.
7. **Q7 — A patched Ruffle (found in review round 4).** GFx-style `$`-key
   text translation (GUI-FONT-004) has no hook in the pinned Ruffle, and the
   workspace ships a patch for it, against GUI-NG-001 as revision 10 worded
   it. Recommended: allow this one narrow patch, to text fields only, with
   its re-vendoring cost recorded (§6 row 9); any other Ruffle patch needs a
   new decision. Alternative: remove the patch and translate by a route that
   leaves Ruffle unpatched. Consequence: GUI-NG-001 stands as written, but
   working code has to be replaced.
   **Decided** (2026-11-27): the recommendation.

---

## 11. Document map

| Need | Go to |
|---|---|
| Host bridge, profiles, catalogs, adapter, navigator, tests | [ui.md](../engine/ui.md) |
| What the vanilla HUDs call and why the meters are empty | [ui.md § Scaleform HUD](../engine/ui.md#scaleform-hud-m486-skyrim--m487-fallout-4) |
| MenuXml evaluator, layout, raster, `MenuProfile` | [ui.md § MenuXml](../engine/ui.md#menuxml-legacy-ui-m484-oblivion--m485-fo3) |
| Oblivion's bed sleep event and frozen attribute picks | [Skill Use § Levels](../engine/skill-use.md#5-levels-7052) and [Oblivion Skill-Use Leveling § Requirements](oblivion-skill-use-leveling.md#5-requirements-by-area) |
| Input focus and modal capture today | [ui.md § Input routing](../engine/ui.md#input-routing) |
| Loading screens | [ui.md § loading screens](../engine/ui.md#original-game-loading-screens-during-scene-transitions) |
| The race menu as built, and what Phase 5 adds | [ui.md § Race menu](../engine/ui.md#race-menu-gui-page-002--003) |
| The Ruffle patch and how to carry it to a new revision | [patch notes](../../third_party/ruffle/PATCHES.md) |
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
| 4 | 2026-11-03 | Owner decisions Q1–Q6 recorded, every one the recommendation | owner: approved |
| 5 | 2026-11-03 | → ACTIVE: parent #6641 and phase issues #6643, #6653, #6661, #6665, #6666, #6667, #6668 filed; the Phase 0 verify items are acceptance criteria of #6643 | — |
| 6 | 2026-11-08 | Phase 0 results (§9.1): inventory with Q1 surfaces, flags, codes (FO3 / FNV 1004 = `HUDMainMenu`), classified catalogs, extension census, HUD feeds, `-s` strings, native-page strings, race-menu wait, Skyrim LE and Starfield; Q2's first question answered; Q5 and Q6 results; §3.1 rows 2, 6, 13, 14, 15 corrected; GUI-HOST-001 (H4), GUI-REG-006 and GUI-FONT-005 reworded; ui.md design section | — |
| 7 | 2026-11-09 | Phase 0 exit: the open verify items each name their owning issue in §9.1 (phase issues, or #6823-#6826 filed for the five no phase issue covered); status line unchanged | — |
| 8 | 2026-11-20 | #7048: GUI-XML-004…007 assign Oblivion's sleep/rest and level-up menus to MenuXml, specify successful and interrupted sleep, and define the frozen attribute-pick contract and acceptance scenarios for the #7047 bindings | — |
| 9 | 2026-11-20 | #7086: implement and exercise the Oblivion MenuXml and native fallback paths for GUI-XML-004…007 in Weynon House. Test cancellation, waiting, and combat interruption without consuming a pending level | — |
| 10 | 2026-11-22 | Review round 3 folded in: §3.1 replaced by links to the checked-in menu inventories, the ui.md registry design and the ROADMAP M48 row, keeping only the rows not yet built, re-measured at HEAD (Scaleform extensions, H1 host behaviour, the `-s` strings, UI frame cost, the race menu's exit, wave 2 and Starfield); §1 dated to the draft; the menu registry defined as built and the Related row linked; §9 records Phases 1, 2 and 4 closed and Phase 3 partly built; the requirement clauses the built phases made untrue dropped or re-pointed at the new §3.1 rows (GUI-REG-001, -002, -005, -007…-011, GUI-HOST-001, -004, GUI-PATH-001, GUI-HUD-001, -004, -005, GUI-GFX-001, GUI-EVT-001, -003, -005, -006, GUI-XML-001, GUI-PAGE-002, -003, -006, GUI-OPS-005); the Tracking line on one line; the status line trimmed | claude: revise |
| 11 | 2026-11-27 | Review round 4 folded in. §9.1 and requirements corrected where refuted: vanilla uses `MenuMode` codes; FNV `VCG01` and FO3 `CG00` also call `ShowRaceMenu`, and no caller proves the call's return timing (GUI-PAGE-002 narrowed, GUI-PAGE-007 added as an engine choice). §3.1 rows 2–4, GUI-HUD-004, GUI-REG-005, line references, resolved verify notes and orphaned owners updated. Starfield moved out of §7.1 scope as Q6 decided (GUI-NG-009); Skyrim LE's gaps owned. Q7 decided: one narrow Ruffle text patch, GUI-NG-001 reworded. One observable per ID (GUI-REG-012, -013, GUI-XML-008…017); §6 rows 9–11; scenarios 8, 9, 11, 12; four stale HEAD citations in §2 and GUI-HOST-002; §8 homes and denominator; Phase 7; design owners; §9.1 shortened | claude: revise |
| 12 | 2026-11-27 | Owner approved revision 11's scope changes: GUI-NG-001 narrowed by Q7; Starfield out of scope (GUI-NG-009); GUI-PAGE-007; host behaviour coverage published in the M48 row; Phase 7 and the split requirement IDs | owner: approved |
