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
4. MCM Helper (`MCM/Config/<mod>/config.json`, `settings.ini`) on the same model. **Built
   (2026-11-23)** as designed in §5.1, with the owner's decisions §6.4–6.6.
   - **Scripting crate.**
     - `crates/scripting/src/mcm/helper/config.rs` parses the config. `helper/settings.rs` holds the
       typed, layered settings store. `helper.rs` lays the page out, reads and writes values, and
       queues actions.
     - The engine scripts are `mcm/MCM_ConfigBase.psc` and `mcm/MCM.psc`, and their static class is
       `fragment/vm/natives/mcm_helper.rs`.
     - An action's function is checked over the installed scripts before it runs
       (`check_installed_function`), so a function the VM cannot run is declined whole.
   - **Binary.**
     - `byroredux/src/mcm_files.rs` reads `mcm\…` through the mod roots and writes
       `<config dir>/mcm/<base master>/<profile>/<mod>.ini`.
     - `--profile-name` names the profile. A boot request's `[mods] name` sets it, and
       `--import-load-order` fills that from an MO2 profile folder.
   - **Measured.**
     - Unit tests for the config (sources, conditions, actions, bad input) and for the settings
       (layers, typed reads, commits only on change, reset).
     - Three VM tests on the engine scripts. One config script registers with the config's name and
       pages; each control kind lays out from its source; and toggle, slider, enum, stepper, key map
       and input write where their values live. Each raises `OnSettingChange` with its id, the group
       switch disables and re-enables its control, and the toggle's action runs after the event. The
       other two cover the error page (bad JSON, `minMcmVersion`, a missing required plugin) and
       `MCM.IsInstalled` / `GetVersionCode` / the global setting calls.
     - The profile directory, path safety and the atomic write.
     - No live run: no MCM Helper mod is available here.
   - **Not built** (§5.1):
     - `RunConsoleCommand` and the key-conflict question.
     - Nested `{…}` translation arguments.
     - A key map shows its scancode, not the key's name.
     - A property or action on a form other than a quest is declined with a diagnostic.
4b. MCM Helper's hotkeys (`keybinds.json`). **Built (2026-11-23).**
   - **Scripting crate.**
     - `crates/scripting/src/mcm/helper/keybinds.rs` parses a mod's hotkeys. Each has an id, a
       description, and an action: `CallFunction`, `CallGlobalFunction`, `SendEvent` or
       `RunConsoleCommand`.
     - It also reads and writes the bindings file, `{ "version", "keybinds": [ { "keycode", "modName",
       "id" } ] }`.
     - A mod's hotkeys become live when its config loads, as MCM Helper reads them in `ReadConfigs`.
   - **Key-map controls.** A key map with an `id` and no value source shows and sets the key bound to
     that hotkey. Its info text without `help` is the hotkey's description. Unmapped (-1) removes the
     binding.
   - **Bindings on disk.**
     - They are layered: any `MCM/Settings/keybinds.json` an earlier install left (read only) under
       the profile's own `keybinds.json`.
     - The profile file is the only one written, with every binding, as MCM Helper commits all of
       them.
   - **Presses.**
     - `byroredux/src/mcm_hotkeys.rs` (`mcm_hotkey_system`) diffs the keys and mouse buttons the
       world holds against the last frame's. Each key going down or coming up is sent as its
       DirectInput code, a mouse button as 256 + its index. The scancode table in `interaction.rs`
       is now the full DirectInput keyboard set.
     - Nothing is sent while an open menu pauses the game, as MCM Helper skips input then.
     - A function call runs on the key going down. `SendEvent` raises `OnControlDown(id)` going down
       and `OnControlUp(id, heldSeconds)` coming up. Each runs after the VM has checked it, as a
       control's action does.
   - **Measured.**
     - Parsing every action kind, bad files, and binding layering with a round trip.
     - One VM test: bind through a key map over an earlier install's bindings, then a press runs the
       call, `SendEvent` runs both events with the held time, a console hotkey and an unbound key run
       nothing, and an unmapped key stops it.
     - The input diff and the key and mouse code tables.
     - No live run.
   - **Not built.**
     - `RunConsoleCommand`: the engine has no game-console interpreter, so it is declined with one
       diagnostic per hotkey.
     - Gamepad buttons.
     - A key the engine itself handles (Escape, the inventory key, the debug `F` / `V`) never reaches
       the held set, so it cannot be a hotkey.
     - `SendEvent` to a form that is not a quest is declined.
5. Fallout 4 MCM (`MCM/Config/<mod>/config.json`, its `MCM.*` Papyrus calls) and New Vegas MCM,
   each after its source is read.

## 5. Other front ends

### 5.1 MCM Helper (slice 4)

Sources, read 2026-11-23 at release v1.6.2 (MIT licence, github.com/Exit-9B/MCM-Helper): the JSON
schemas `docs/config.schema.json` and `docs/keybinds.schema.json`, the Papyrus API
`scripts/public/MCM.psc` and `MCM_ConfigBase.psc`, and the plugin source that gives them meaning
(`ConfigStore`, `SettingStore`, `Config/ValueSource`, `Config/Control`, `Config/Action`,
`Config/GroupConditionTree`, `Papyrus/MCM_ConfigBase`). v1.6.2's version code
(`MCM.GetVersionCode()`) is 15.

**What MCM Helper is.**

- A config script `extends MCM_ConfigBase`, which `extends SKI_ConfigBase`. `MCM_ConfigBase`
  overrides `OnPageReset` and every `OnOption…` event with natives, so the page is built from
  `Data/MCM/Config/<mod>/config.json`. `<mod>` is the file stem of the plugin that defines the config
  quest. The plugin sets the script's `Pages` from the config's page names and `ModName` from its
  `displayName`. A config that does not parse gives one "Error loading config" text option, whose
  select shows the parse error.
- `config.json`:
  - Top level: `modName`, `displayName`, `minMcmVersion`, `pluginRequirements`,
    `cursorFillMode` (`leftToRight` or `topToBottom`), and `content` or `customContent` for the front
    page.
  - `pages`: each has a `pageDisplayName` and its own `content` or `customContent`.
  - A control has `type`, `id`, `text`, `position`, `help` (where `{value}` is replaced by the value),
    `groupCondition` and `groupBehavior` (`disable`, `hide` or `skip`), `action`, and `valueOptions`.
  - Control types: `empty`, `header`, `text`, `toggle`, `hiddenToggle` (holds a value, never drawn),
    `slider`, `stepper` (a text option that cycles through its options on select), `menu` (a string
    value from `options` and `shortNames`), `enum` (an index into `options`), `color`, `keymap`
    (`ignoreConflicts`) and `input`.
- Where a value lives (`valueOptions.sourceType`):
  - `PropertyValueBool`, `PropertyValueInt` or `PropertyValueFloat`: `propertyName` on `scriptName`
    attached to `sourceForm` (`"Plugin.esp|D62"`). Both default to the config quest and its script.
  - `ModSettingBool`, `ModSettingInt` or `ModSettingFloat`: the control's `id`, spelled
    `key:Section`.
  - `GlobalValue`: a `GLOB`.
  - Text, menu and input controls use a string property or a string mod setting.
  - `defaultValue` is what the reset key restores. A mod setting without one falls back to the
    `settings.ini` default.
- Mod settings:
  - Defaults come from `MCM/Config/<mod>/settings.ini`. The user's values come from
    `MCM/Settings/<mod>.ini`, which overrides them.
  - A key's first letter gives its type: `b` bool, `i` int, `u` unsigned, `f` float, `s` string, `r`
    colour.
  - A write commits only that key to the user's file.
- Events:
  - A change runs `OnSettingChange(id)` on the config script, then the control's action.
  - A toggle with `groupControl N` makes group N active while it is checked, and changing it refreshes
    the page.
  - A `groupCondition` is a group number, an array (OR by default), or `{OR | AND | ONLY | NOT: …}`.
  - `OnPageSelect(page)` follows each page reset.
- Actions:
  - `CallFunction` calls `function` on `scriptName` attached to `form`.
  - `CallGlobalFunction` calls `script.function`.
  - In `params`, `{value}` becomes the control's value, and the prefixes `{i}`, `{u}`, `{b}`, `{f}`,
    `{s}` and `{r}` type a literal.
- Papyrus API:
  - `MCM.IsInstalled()`, `MCM.GetVersionCode()`, and `MCM.Get/SetModSetting{Int,Bool,Float,String}(mod,
    name)`.
  - On the base: `RefreshMenu()`, `SetMenuOptions(id, options, shortNames)`,
    `Get/SetModSetting…(name)` and `GetCustomControl(keyCode)`.
- `keybinds.json` (with the user's `MCM/Settings/keybinds.json`):
  - Hotkeys with a description and an action. Besides the two calls, an action can be `SendEvent`
    (`OnControlDown` / `OnControlUp`) or `RunConsoleCommand`.
  - A `keymap` control with no value source binds such a hotkey by its `id`.
- Texts are `$`-keys. MCM Helper's translator also fills nested `{…}` arguments
  (`$MCM_KeyAlreadyMapped{…}`).

**Design.**

- **Engine scripts.** The engine supplies `MCM_ConfigBase` and `MCM`, shadowing MCM Helper's scripts as
  §3.2 does SkyUI's.
  - `MCM_ConfigBase` extends the engine `SKI_ConfigBase`. Its overridden events call a static class
    `McmHelper`, whose VM natives build the page into `McmHost` through the same page buffer and add
    options as a SkyUI script would.
  - A native that changes a value returns the changed control's `id`. The Papyrus body then raises
    `OnSettingChange(id)`, and the control's action runs after the event as a call the host queues.
- **The store.** `McmHelperStore` is a resource built on first use per mod:
  - each mod's parsed `config.json`;
  - its `settings.ini` defaults and user values;
  - the open page's cache: option id to control, `SetMenuOptions` overrides, and the group toggles.
- **Reading files.** MCM Helper reads loose files only (`std::filesystem`), so the engine reads
  `mcm\config\…` and `mcm\settings\…` through the profile's mod roots (`LayeredSources` with no
  archive).
- **Value sources.**
  - A property on a quest reads and writes that quest's script variables (`QuestScriptVariables`,
    `QuestScriptObjects`). A property on another kind of form is declined with a diagnostic.
  - `GlobalValue` uses `Globals`.
  - Mod settings use the store.
- **Actions.** `CallFunction` runs on a quest's own scripts, and `CallGlobalFunction` runs any global
  the VM checks. Any other target form is declined with a diagnostic.
- **Version probes.**
  - `MCM.IsInstalled()` returns true and `GetVersionCode()` returns 15, so a mod gating on MCM
    Helper finds it (LMC-SCR-010).
  - A config with `minMcmVersion` above 15, or with a `pluginRequirements` entry not in the load
    order, gets MCM Helper's own error page.

**Not in this slice** (each its own follow-up): `keybinds.json` hotkeys and the key-map controls
bound to them (they need an input hook that runs Papyrus on a key press); `RunConsoleCommand`;
the conflict question a key map asks (the engine's control bindings are not consulted, §4); nested
`{…}` translation arguments.

**Settings on disk (§6.4, §6.5).**

- The player's values are written to `<config dir>/byroredux/mcm/<game>/<profile>/<mod>.ini`.
  - The config dir is `settings.toml`'s.
  - `<profile>` is `--profile-name`. A boot request's `[mods] name` sets it, and `--import-load-order`
    fills that from the MO2 profile's folder name. Without one it is `default`.
  - So settings survive saves, new games and mod updates, and two named profiles keep their own
    (LMC-SET-004).
- Reads are layered:
  1. the mod's `MCM/Config/<mod>/settings.ini` defaults;
  2. any existing `MCM/Settings/<mod>.ini` an earlier MCM Helper install left in the game's `Data` or
     a mod root (read, never written);
  3. the profile's own file.

  A write commits only the changed key, to the profile's file, as MCM Helper commits to its own.

Decisions: §6.4–6.6.

### 5.2 Fallout 4 MCM and New Vegas MCM (sketch)

Fallout 4 MCM describes pages declaratively in `config.json` and binds controls to script properties,
globals or INI keys; MCM Helper was modelled on it, so its front end should reuse §5.1's store. New
Vegas' MCM is a MenuXml menu driven by script variables through NVSE functions; it is its own front
end on the MenuXml track. Each needs its documented format read first; neither is designed here.

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
4. **Where MCM Helper's settings live** (owner, 2026-11-23). Taken: one INI per mod in an engine
   directory per game and named profile (§5.1). The alternatives were the save (a new game would lose
   them, unlike MCM Helper) and the game's `Data/MCM/Settings` (written into the install, shared by
   every profile, against LMC-SET-004). Keying by the profile fingerprint was rejected because it
   hashes every plugin's bytes, so any mod update would reset every mod's settings.
5. **Settings an earlier MCM Helper install left** (owner, 2026-11-23). Taken: read as a layer
   between the defaults and the profile's values, never written.
6. **MCM Helper's hotkeys** (`keybinds.json`; owner, 2026-11-23). Taken: a follow-up slice (4b),
   because they need an input hook that runs Papyrus on a key press. Built the same day (§4, 4b).
