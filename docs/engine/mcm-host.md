# MCM Host — Design

**Status**: DESIGN (2026-11-23); the owner took every §6 recommendation the same day. Slices 1 and 2 are built. Requirement:
[LMC-UI-002](../prd/legacy-mod-compatibility.md) (catalog row U2, D4 "served by an engine-owned MCM
host"), Phase 3 (#6672). Nothing here is built.

The engine owns one Mod Configuration Menu model. Four front ends feed it (SkyUI's `SKI_ConfigBase`
Papyrus contract, MCM Helper's JSON, Fallout 4 MCM's JSON, New Vegas' MCM), and one presentation
draws it. This document designs the model, the SkyUI front end and the first presentation; the other
three front ends reuse the model and are sketched in §5.

## 1. What the SkyUI contract is

Sources: the SkyUI wiki's *MCM Quickstart*, *MCM API Reference*, *MCM Advanced Features* and
*MCM State Options* pages (github.com/schlangster/skyui/wiki), read 2026-11-23.

- A mod ships a quest whose script `extends SKI_ConfigBase`, with properties `ModName` and
  `Pages` (`string[]`), and a reference alias `PlayerAlias` (the player) carrying
  `SKI_PlayerLoadGameAlias`, which forwards the game-load event to the config script.
- Lifecycle events on the config script: `OnConfigInit()` (once per save, before registration),
  `OnConfigRegister()`, `OnVersionUpdate(int version)` when `GetVersion()` differs from the version
  the save recorded (a first sight runs `OnConfigInit` then `OnVersionUpdate`), `OnGameReload()`
  (a script overriding it must call `parent.OnGameReload()`), `OnConfigOpen()` and `OnConfigClose()`.
- A page is built on demand: on every mod or page selection the panel clears and calls
  `OnPageReset(string page)` (`""` for the mod's front page). The script lays out up to 128 options
  on a 2-column by 64-row grid with `SetCursorPosition(0..127)` and
  `SetCursorFillMode(LEFT_TO_RIGHT | TOP_TO_BOTTOM)` (n, n+1 / n, n+2), and
  `AddHeaderOption`, `AddEmptyOption`, `AddTextOption`, `AddToggleOption`, `AddSliderOption`,
  `AddMenuOption`, `AddColorOption`, `AddKeyMapOption`, `AddInputOption`, each returning an option
  id, or their state variants `Add…OptionST(stateName, …)`, whose events run in the named state
  (`OnSelectST`, `OnSliderAcceptST(value)`, …).
- The panel never changes a value itself. Interaction raises an event (`OnOptionSelect`,
  `OnOptionSliderOpen` / `Accept(value)`, `OnOptionMenuOpen` / `Accept(index)`,
  `OnOptionColorOpen` / `Accept(0xRRGGBB)`, `OnOptionKeyMapChange(keyCode, conflictControl,
  conflictName)`, `OnOptionInputOpen` / `Accept(text)`, `OnOptionHighlight`, `OnOptionDefault`), and
  the script answers with `Set…OptionValue`, `SetOptionFlags`, the dialog setters
  (`SetSliderDialogStartValue` / `DefaultValue` / `Range` / `Interval`, `SetMenuDialogOptions` /
  `StartIndex` / `DefaultIndex`, `SetColorDialog…`, `SetInputDialogStartText`), `SetInfoText`,
  `SetTitleText`, `ShowMessage(...) -> bool` (blocks like `Message.Show`), `ForcePageReset()` and
  `LoadCustomContent` / `UnloadCustomContent` (a `.swf` or `.dds` drawn in place of the list).
- Flags: `OPTION_FLAG_NONE`, `_DISABLED`, `_HIDDEN`, `_WITH_UNMAP` (keymap unmap button).
- Settings live in the mod's own script variables, which the save already carries
  (`QuestScriptVariables` / `QuestScriptObjects`); the wiki's versioning rules ("only add
  variables") are about exactly that.

## 2. What the engine has today

Measured 2026-11-23:

- The fragment VM resolves `extends` chains through any mod's `.pex` (`fragment/vm/script_model.rs`
  `script_chain`), and treats a script that declares a `Native` function as an engine class.
- SkyUI's own `SKI_ConfigBase.pex` talks to its `configmenu.swf` through SKSE `UI.Invoke*` /
  `UI.Set*`, which the SDK classifies `Unsupported` by policy ("arbitrary Scaleform object access
  and menu mutation are not exposed", `sdk/src/compatibility/source_alias.rs`). Only
  `UI.IsMenuOpen` is served. The checker therefore declines SkyUI's base script, and with it every
  config script that extends it.
- **Quest scripts never received `OnInit`** (before slice 1, §4). `QuestScriptInstances` serves inlining and calls only;
  reference and alias scripts get `OnInit`, a quest's own VMAD scripts do not. The CK wiki's
  *OnInit* page: quest and alias scripts receive it "on game startup, and again whenever the quest
  starts, due to the quest being reset" (once when the quest is Run Once).
- Mod events (`RegisterForModEvent` / `SendModEvent`) exist only for provider programs on placed
  references, not in the fragment VM.
- The Journal menu movie is not opened (it is recorded `S4` with unserved host calls), and there is
  no `skse` ActionScript global. The native Settings page (`crates/native-ui`, `GameMenuPage::Settings`)
  draws toggle, slider and choice controls; extension manifests declare up to 64 settings there.

## 3. Design

### 3.1 The model (`McmHost`, a resource)

- **Registry**: one entry per config script instance (quest FormID, as a stable identity), with its
  `ModName`, `Pages`, the version the save recorded, and whether `OnConfigInit` has run. Saved, so
  `OnVersionUpdate` fires only when `GetVersion()` changes; the entry is the engine's version of the
  state SkyUI's base kept in its own variables.
- **Page buffer** (not saved): the open mod and page, 128 cells, the cursor and fill mode, each
  cell's option (kind, label, value text, numeric value, flags, option id or state name), title and
  info text, the open dialog's parameters, custom content. Option ids are cell indices offset by the
  page generation, so a stale id from a previous page reset matches nothing.
- **Events out, values in**: presentation actions become queued events on the owning script (by
  option id, or in the option's state for an `ST` option); the script's setters write the buffer.
  A page reset is the only way the list is rebuilt.

### 3.2 The SkyUI front end: an engine `SKI_ConfigBase`

The engine supplies `SKI_ConfigBase` and `SKI_PlayerLoadGameAlias` itself, as engine classes whose
functions are natives bound to `McmHost`; they shadow SkyUI's `.pex` of the same names, the way
LMC-FIX-004's stand-ins shadow a skipped patch. A config script then resolves its parent to the
engine class: `AddToggleOption` writes the page buffer, `parent.OnGameReload()` reaches the engine's
body, `ModName` / `Pages` / `CurrentPage` are properties the engine reads. SkyUI's other scripts and
movies (inventory, favourites, its widgets) are unaffected and stay D1 through the Ruffle host.

Required in the VM:

1. **Quest-script `OnInit`** at game startup and on quest start (reset), once for Run Once quests,
   per the CK wiki. This is not MCM-specific: every Skyrim quest script that initialises itself in
   `OnInit` is silently skipped today.
2. **Engine-class natives for the fragment VM**, a route from a native call on an engine class to an
   engine service (the provider catalog has one; the VM's native bridge does not take providers).
3. **`ShowMessage` blocking**, as `Message.Show` already blocks (built: a box wrapper's box is the
   calling statement's, §4 slice 2).

### 3.3 The first presentation: a native page

A **Mod Configuration** entry in the native pause menu: the registered mods, then a mod's pages,
then the page's 2-column grid drawn with the native page's controls (toggle, slider, choice exist;
text, header, empty, colour, key-map and input are added), the info text below, and the slider,
menu, colour, key-map and input dialogs. The SkyUI movie (`configmenu.swf`) is not used: driving it
needs the Scaleform object access the SDK declines. Custom content (`LoadCustomContent`) draws a
`.dds`; a `.swf` custom panel is reported unsupported until a Ruffle sub-movie host exists.

Console commands drive the same model headless: `mcm.list`, `mcm.open <mod> [page]`,
`mcm.select <option>`, `mcm.slider <option> <value>`, `mcm.menu <option> <index>`,
`mcm.input <option> <text>`, `mcm.key <option> <code>`. The smoke gate and the corpus scenario
(§7.3's *SkyUI — open the inventory, the MCM lists every registered mod page*) use these.

### 3.4 Persistence

SkyUI MCM settings need nothing new: they are the mod's script variables. The registry entry (§3.1)
is saved. MCM Helper and Fallout 4 MCM settings, which live outside scripts, go through the
extension-state save path (STATE-001) as LMC-UI-002 says.

## 4. Slices

1. Quest-script `OnInit`. **Built (2026-11-23)** for the CK's first rule: a quest's own scripts are
   checked a few per frame (running quests first; `check_quest_script`, `QuestOwnScripts`) and
   receive `OnInit` once per save (`quest_script_init_system`, gated on the saved `::init::` key),
   so a quest a mod adds to an existing save gets it too. On vanilla Skyrim SE (Whiterun, 900
   frames) 151 quests' scripts check; `FavorChangeLocationQuestScript`, the Civil War controllers
   and the `defaultAddItem*` initialisers run it (the last only copy their properties). The second rule, a non-Run-Once quest's reset as it starts (variables and properties back to
   their defaults, then `OnInit` again), is built in the fragment dispatch, before the quest's
   startup stage (PRD revision 35).
2. `McmHost` model, the engine `SKI_ConfigBase` / `SKI_PlayerLoadGameAlias`, the VM native route,
   registration and version events, the `mcm.*` console driver. **Built (2026-11-23).**
   - `crates/scripting/src/mcm/SKI_ConfigBase.psc` is the engine's base, Papyrus written from the
     wiki with SkyUI's published constants (option flags 1 / 2 / 4, `LEFT_TO_RIGHT` 1,
     `TOP_TO_BOTTOM` 2, option ids `cell + (page + 1) * 0x100`); `mcm::engine_script` shadows a
     mod's script of the same name in the provider library. Its option calls are the static class
     `McmHost` (`fragment/vm/natives/mcm.rs`), which writes [`McmHost`](../../crates/scripting/src/mcm.rs).
   - The panel's actions run its `_McmEvent`, which dispatches as SkyUI does: a state option's event
     in its state, then the previous state back. The quest-script check covers `OnGameReload` and
     `_McmEvent`, so a config script's handlers are checked before any runs.
   - Saved state is the config script's own variables (its settings, the base's initialised flag
     and `CurrentVersion`); `McmHost` is per session, and `mcm_registration_system` raises
     `OnGameReload` on every config script whose `OnInit` ran in an earlier session.
   - `mcm.list / open / page / show / select / accept / cancel / default / highlight / close`
     drive it from the console.
   - Measured by a unit test with a config script on the engine base (registration from `OnInit`,
     a toggle by id, a slider by state with its dialog, a stale id, re-registration after a load
     without a second `OnConfigInit`). No live run: the engine loads Papyrus as compiled `.pex`,
     and no Papyrus compiler or SkyUI-dependent mod is available here.
   - `ShowMessage` (2026-11-23, PRD revision 35): the engine's is a box wrapper, a function whose
     body is one `Return`, here `Return McmHost.ShowMessage(...)`. Called for its value, it hands
     its box to the calling statement, which waits and then replays with the answer, the way it
     does for a `Message.Show()` written in place, so `If ShowMessage("Reset?")` works. The box
     carries the message and the accept (and cancel) labels; accepted is button 0.
   - Not built: key-map conflict detection (`GetCustomControl`, the conflicting control's name)
     passes "" (no conflict); a `.swf` custom content panel.
3. The native Mod Configuration page and its dialogs. **Built (2026-11-23).**
   - `crates/native-ui/src/mcm.rs` draws the registered mods and the open mod's pages, the page's
     two-column grid (header, empty, text, toggle, slider, menu, colour, key-map and input
     options, with the disabled and hidden flags), the info text, and the slider, menu, colour,
     key-map (with unmap), input and message dialogs. It reads `McmSnapshot` and hands back
     `McmAction`s (`views.rs`); it never writes a value. The pause page offers it once a mod
     registered, and the menu registry counts it as the pause menu. Its words are labels, the
     snapshot's data or rows of `native-page-strings.tsv` (pages `mod configuration`, `pause`).
   - `byroredux/src/mcm_page.rs` is the adapter: `McmHost` to `McmSnapshot` (cells in grid order with
     SkyUI's option ids, the dialog with its option's own format and flags), every script text
     through the Scaleform `$`-key table (`byroredux_ui::translate_text`, installed once from the
     interface archive and the enabled plugins' translation files, since the page has no movie of
     its own), and each action to its driver function; leaving the menus closes an open page.
   - Measured by 16 click-driven page tests (each interaction yields the action the contract
     needs and nothing else), the page-strings gate (every drawn word is the game's, the
     snapshot's or listed) and 8 adapter tests. No live run: no SkyUI-dependent mod is available
     here, so the path from a click to a config script's event is covered in two halves (the
     page's actions; slice 2's drivers on a generated config script), not end to end.
   - A script message box (`ShowMessage`, or any box a script raises while a config page is open)
     shows on the page itself, over any dialog: its title, text and buttons come from the shared
     message queue (`$`-keys translated), a button answers it by index through the same path
     `ui.answer` drives, and no other surface opens for it meanwhile (a native page opened over the
     Mod Configuration page would draw under it, or wait unseen). Skyrim box text and buttons on the
     native page are translated the same way. Not verified live: where the page is not open, the
     existing movie or native surface still decides.
   - Not built: a `.dds` custom panel is carried but not decoded (the page reports it as
     unsupported, like a `.swf`).
4. MCM Helper (`MCM/Config/<mod>/config.json`, `settings.ini`) on the same model.
5. Fallout 4 MCM (`MCM/Config/<mod>/config.json`, its `MCM.*` Papyrus calls) and New Vegas MCM,
   each after its source is read.

## 5. Other front ends (sketch)

MCM Helper and Fallout 4 MCM describe pages declaratively in `config.json` and bind controls to
script properties, globals or INI keys; the model's cells hold them unchanged, and their writes go to
the bound target. New Vegas' MCM is a MenuXml menu driven by script variables through NVSE
functions; it is its own front end on the MenuXml track. Each needs its documented format read
first; none is designed here.

## 6. Decisions (owner, 2026-11-23: each recommendation taken)

1. **The SkyUI front end.** Recommended: the engine supplies `SKI_ConfigBase` (§3.2), shadowing
   SkyUI's. The alternative runs SkyUI's own base script and `configmenu.swf`, which means serving
   SKSE's `UI.Invoke*` / `UI.Set*` / `UI.Get*` (arbitrary Scaleform access) and opening the Journal
   movie, reversing the SDK's UI policy.
2. **The presentation.** Recommended: the native page first (§3.3). The alternative is SkyUI's
   config panel movie driven by the engine model, which needs decision 1's alternative anyway.
3. **Where MCM lives in the menus.** Recommended: an entry in the native pause menu on every game
   with an MCM front end. SkyUI puts it in the Journal (System tab); the engine's Journal is the
   native pause page already.
