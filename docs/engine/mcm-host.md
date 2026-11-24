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
5. Fallout 4 MCM (`MCM/Config/<mod>/config.json`, its `MCM.*` Papyrus calls and F4SE external
   events). **Built (2026-11-23)** as designed in §5.2. New Vegas' MCM is its own slice after it
   (§5.3; owner, 2026-11-23).
   - **Scripting crate.**
     - `config.rs` gains a `Dialect` (`parse_in`) with FO4's control names and `button`.
     - `helper.rs`'s page and option functions take an `McmOwner`: a config script's quest, or a
       config by mod name. A method action without `scriptName` calls its form's own script.
     - `McmHelperStore::for_fallout4` sets the dialect and version code 9, and holds the configs found.
       It skips an earlier install's `Keybinds.json`.
     - `register_config_menus` (run by `mcm_registration_system`) lists them in `McmHost`
       (`McmMod::config`), whose drivers answer their panel actions through `raise_config`.
     - `fragment/external_events.rs` keeps the per-session registrations that
       `RegisterForExternalEvent` / `UnregisterForExternalEvent` make. They're checked as statements
       with a literal callback the script has. `send_external_event` delivers them.
   - **Binary.**
     - `mcm_files.rs` lists the config folders through `LayeredSources::list` when the base master is
       `Fallout4.esm`.
     - A load clears the registrations (`discard_outgoing_session_requests`).
   - **Measured.** One VM test:
     - the config is listed and another, whose plugin is missing, is not;
     - version code 9;
     - every FO4 control kind is laid out, with the image left out, the slider at its step's
       decimals, and text with no value;
     - a failed group hides;
     - a listener quest hears the opening, each change with mod and id, and the closing;
     - the dropdown writes a property, and the button's action runs;
     - a load clears the host and the registrations, and the config lists itself again.

     Two more tests cover the registration's check and the event filter. Unit tests cover the folder
     listing. No live run.
6. New Vegas' MCM (§5.3): designed, not built. Two inputs are needed first:
   - real NVSE-compiled plugins, to derive the byte forms from (§6.9);
   - the MCM Guide v6, for the tile-tree addresses the stand-in must answer (§5.3).

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

### 5.2 Fallout 4 MCM (slice 5)

**Sources** (read 2026-11-23). FO4 MCM (reg2k and Neanka) publishes no licence for its plugin
source, so the engine follows the author documentation only:

- the wiki of github.com/Neanka/MCM_0.1_AS3 (*Home*, *Menu-Layout*, *Control-Types*,
  *Setting-Types*, *Display-Conditions*, *Hotkeys*, *Advanced-Papyrus-Integration*, *Translations*);
- the shipped `MCM.psc`, `changelog.txt` and `settings.ini` of github.com/reg2k/f4mcm.

The C++ source was not read. Release 1.40's version code (`GetVersionCode`) is 9.

**What differs from MCM Helper** (§5.1, which was modelled on it):

- **No config script.** A menu is a folder `Data/MCM/Config/<mod>/` with a `config.json`, whose
  `modName` must match the folder. It is found by the folder, not registered from Papyrus. The
  `pluginRequirements` key gates it.
- **Controls by FO4's names.**

  | FO4 control | MCM Helper equivalent |
  |---|---|
  | `spacer` | `empty` |
  | `section` | `header` |
  | `switcher` | `toggle` |
  | `hiddenSwitcher` | `hiddenToggle` |
  | `dropdown` (an index into `options`) | `enum` |
  | `hotkey` | key map bound to a `keybinds.json` id |

  - `text`, `slider` and `stepper` keep their names.
  - New: `button`, whose select runs its `action`.
  - New: `image`, an AS3 class from the mod's `lib.swf`.
  - New: `positioner` and `textinput`, which are listed but undocumented.
  - There is no `color`, `menu`, `input`, `position`, `cursorFillMode`, `customContent`,
    `formatString`, `shortNames` or `defaultValue`.
- **Group conditions.** They have no `NOT` and no `groupBehavior`. The docs call a control whose
  condition fails "hidden".
- **Events.** `RegisterForExternalEvent("OnMCMSettingChange|<mod>", "Callback")`, an F4SE function on
  any script, receives `(modName, id)` after a change. Without the `|<mod>` suffix it receives every
  mod's changes. `OnMCMMenuOpen` and `OnMCMMenuClose` receive `(modName)`. The wiki also registers a
  bare `OnMCMOpen`. Scripts register again after every load.
- **Hotkeys.**
  - Same `keybinds.json` and actions as §5.1.
  - Bindings may carry Ctrl, Alt and Shift modifiers.
  - The player's bindings file, `Data/MCM/Settings/Keybinds.json`, is undocumented, and two
    third-party sources disagree on its key-code space.
- **Papyrus API.** `MCM.psc` is `Scriptname MCM Native Hidden`, with global natives:
  `IsInstalled`, `GetVersionCode`, `RefreshMenu`, and `Get/SetModSetting{Int,Bool,Float,String}(mod,
  name)`.

**Design.**

- **Same store.** The front end is §5.1's store under a Fallout 4 dialect.
  - `config.rs` maps FO4's control names onto the same kinds and adds `button`.
  - A failed group condition hides the control.
  - A slider shows its value to its step's decimals.
  - `image`, `positioner` and the numeric `textinput` variants are left out of the page, with one
    diagnostic each. `textinput` is a string input, bound as §5.1's input is.
- **Menus with no script.**
  - The engine lists `mcm\config\*\config.json` through the profile's mod roots on Fallout 4.
  - It registers each config whose `pluginRequirements` are loaded, and whose own plugin
    (`<modName>.esp/.esm/.esl`) is loaded when it names no requirement.
  - Each becomes an `McmHost` entry with no quest. The panel's actions reach the store directly
    instead of a config script's `_McmEvent`. The menu re-registers after a load, as a config
    script does.
- **External events.**
  - The VM serves `RegisterForExternalEvent(name, callback)` and `UnregisterForExternalEvent(name)`.
    They must be statements with a literal callback, which the check covers.
  - Registrations are kept per session, as F4SE keeps them, and cleared by a load.
  - A change sends `OnMCMSettingChange(modName, id)`. Opening a menu sends `OnMCMMenuOpen(modName)`
    and also the wiki's bare `OnMCMOpen()`; closing it sends `OnMCMMenuClose(modName)`. Each event
    goes to the registrations for the bare name and for `name|modName`.
- **`MCM` script.** The engine `MCM` gains `RefreshMenu()`, which refreshes the open page.
  `GetVersionCode()` returns 9 on Fallout 4 and 15 elsewhere.
- **Hotkeys.** A `hotkey` control is §5.1's id-bound key map. A binding has no modifiers. An earlier
  FO4 MCM install's `Keybinds.json` is not read, because its key codes are undocumented. Bindings
  live in the profile's `keybinds.json` (§6.4).
- **Not built:**
  - `image` (an AS3 class) and `positioner`;
  - modifier keys on a hotkey;
  - an earlier install's hotkey bindings;
  - the `textinputInt` and `textinputFloat` variants, which only third-party sources describe.

### 5.3 New Vegas MCM (slice 6)

**Sources** (2026-11-23):

- the GECK wiki dumps (`/mnt/data/src/reference/geckwiki`, `geck-uesp-wiki`): *MCM*, *GetMCMFloat*,
  *SetMCMFloat*, *SetMCMString*, *GetModINISetting*, *SetModINISetting*, *GetUIFloat*, *SetUIFloat*,
  *SetUIString*, *SetUIStringEx*, the string-variable tutorial and the `let` / `sv_*` pages, for
  behaviour;
- two published mod scripts that drive MCM (Tale of Two Wastelands' `TTWMCMSCPT`, DarnifiedUI's
  config sub-menu), for the protocol as mods use it;
- secondary notes that cite the MCM Guide (NVForever's spec notes).

The MCM Guide v6 itself (Nexus) has not been read; points that only it settles are marked
**guide: verify**. MCM is closed source and not redistributable.

**The protocol.**

- **Registration.** A mod's quest script, in `GameMode` on `GetGameRestarted`, checks `IsModLoaded
  "The Mod Configuration Menu.esp"`. It then `ListAddForm`s one MiscItem (its line's name) to MCM's
  form list, `BuildRef (GetModIndex "The Mod Configuration Menu.esp") 2790` (0xAE6). The list is not
  saved; mods add again each session.
- **The frame loop.** While the pause menu is open, the mod's `Begin MenuMode 1013` block:
  - acts only when `GetMCMFloat 0 0 "_ActiveMod"` is its own mod index;
  - reads `_ActiveOption` and `_Value`;
  - answers at most one request a frame, each a trait on `StartMenu/MCM` that MCM sets and the mod
    clears.

  | Request | The mod's answer |
  |---|---|
  | `_Reset` | Lays the page out on `MCM_Options/Option1..36`: `_enable`, `_title`, `_type`, `_value`, `_prefix`, `_suffix`, `_textOn` / `_textOff` and so on, plus `_columns`. |
  | `_Default` | Writes its defaults, then sets `_Reset`. |
  | `_NewValue` | Takes `_Value`, or `_Value1..3`. |
  | `_ShowList` | Fills the list window, `MCM_List/Item1..10`. |
  | `_ShowScale` | Fills the slider window: `_Value`, `_ValueMin`, `_ValueMax`, `_ValueIncrement`, `_ValueDecimal`. |
  | `_DefaultScale` | Puts the slider's default in `_Value`. |

  Mouse-over sets `MCM_Options/_optionID`, and the mod writes the info line.
- **Option types.**

  | `_type` | Option |
  |---|---|
  | 0 | Title |
  | 1 | List |
  | 2 | Whole-number slider |
  | 2.5 | Decimal slider |
  | 3 | Key |
  | 4 | Off/on |
  | 5 | Check box |
  | 6 | Custom off/on text |
  | 7 | Fixed text |
  | 8 | Two or three sliders |
  | 9 | Colour |

  Sub-menus `_ActiveSubMenu` go up to 10.
- **Settings** are the mod's own script variables and globals, so they are saved. Optionally they go
  to `Data\Config\<File>.ini` through `GetModINISetting` / `SetModINISetting`.

**Design (owner's decisions §6.7–6.10).**

- **The engine stands in for MCM** (§6.7, like LMC-FIX-004's stand-ins).
  - `IsModLoaded`, `GetModIndex` and `BuildRef` answer for "The Mod Configuration Menu.esp" with an
    engine slot.
  - `ListAddForm` into its form list registers a mod line.
  - A real MCM in the load order is not run: its scripts and XML are replaced, and the preflight
    reports it.
- **The tile tree is data.** `StartMenu/MCM` is a trait store (`NvMcmTraits`): paths in the tree's
  own names and indices, as `GetUIFloat` / `SetUIFloat` / `SetUIString(Ex)` and MCM's `*MCMFloat` /
  `SetMCMString` address them. `*:N` child order is **guide: verify** against a real menu.
  The published scripts already disagree with the secondary notes on where traits live:
  - TTW reads the mouse-over option as `GetMCMFloat 0 1 "_optionID"`, where the notes say `1 0`.
  - Both scripts address MCM's children by position only (`*:5/SubMenu1`, `*:8/SubTitle1`,
    `image/visible`).

  The stand-in must answer at exactly those addresses, so the trait store and the front end wait for
  the guide as well.
- **A third front end on `McmHost`.** Each registered line is a menu.
  1. Opening one sets `_ActiveMod`, `_ActiveMenu` and `_Reset`.
  2. The mods' `MenuMode 1013` blocks run while the Mod Configuration page is open; it counts as the
     pause menu, `StartMenu`.
  3. The traits they write become the page's cells.
  4. The panel's actions become the request traits (`_NewValue` with `_Value`, `_ShowList`,
     `_ShowScale`, `_Default`).
- **NVSE in the ObScript VM** (§6.8–6.9).
  - It gets the commands MCM menus use: `IsModLoaded`, `GetModIndex`, `BuildRef`, `ListAddForm`,
    `GetGameRestarted`, `GetGameLoaded`, `GetUIFloat`, `SetUIFloat`, `SetUIString`,
    `SetUIStringEx`, the `sv_*` family, `let` / `eval` and `$`.
  - It gets NVSE string variables. A string variable is a local whose number is an id into a string
    table, which is saved with the session instead of NVSE's co-save.
  - Its byte forms (opcodes, the expression tokens of `let` / `eval`, format-string arguments, the
    string-variable encoding) are **derived from real NVSE-compiled plugins**: each script's `SCTX`
    source is aligned with its `SCDA` bytes, as the VM's vanilla format was. xNVSE's headers name
    what the bytes show.
  - Until such plugins are available, this part is not built.
- **MCM.dll's commands** (§6.10). The five the GECK wiki documents are built from it: `GetMCMFloat`,
  `SetMCMFloat`, `SetMCMString`, `GetModINISetting` and `SetModINISetting`. `SetMCMFloatMass`,
  `SetMCMStringMass`, `SetMCMModList` and `GetMCMListWidth` are empty wiki stubs: a call is declined
  with a diagnostic until the MCM Guide documents them. `SetMCMModList` builds MCM's own list, which
  the stand-in replaces.
- **`GetModINISetting` / `SetModINISetting`.** They read `Data\Config\<File>.ini` through the mod
  roots and write the profile's copy. This follows §6.4–6.5: per game and named profile, with an
  earlier file read but never written.

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
7. **New Vegas MCM itself** (owner, 2026-11-23). Taken: the engine stands in for MCM (§5.3). The
   alternative needed the player's MCM install: running its scripts and rendering its
   `start_menu.xml` on the MenuXml track.
8. **How much NVSE the slice builds** (owner, 2026-11-23). Taken: the commands MCM menus use, and
   NVSE string variables (`string_var`, `let`, `$`, `sv_*`).
9. **Where NVSE's byte forms come from** (owner, 2026-11-23). Taken: derived from real NVSE-compiled
   plugins (`SCTX` aligned with `SCDA`). Building from xNVSE's headers plus the layout nv-rs (a
   GPL-3.0 reimplementation) states was not taken.
10. **MCM.dll's undocumented commands** (owner, 2026-11-23). Taken: declared gaps. The four commands
    the GECK wiki leaves as stubs are declined until the MCM Guide documents them; the description
    derived from a disassembly of MCM.dll is not used (the no-decoding rule).

