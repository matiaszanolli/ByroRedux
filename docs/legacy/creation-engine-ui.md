# Creation Engine UI System

The Creation Engine (Skyrim through Fallout 4) uses Scaleform GFx — a
proprietary Adobe Flash runtime — for all in-game user interface menus.
Scaleform was already discontinued technology before Skyrim shipped in 2011;
Bethesda continued using it through Fallout 4 (2015) before finally replacing
it with a custom UI framework in Starfield.

Source: [Fallout 4 Creation Kit Wiki — User Interface](https://falloutck.uesp.net/wiki/Category:User_Interface)

---

## Architecture

```
┌─────────────────────────────────────────────────┐
│  Papyrus Scripts                                │
│  UI.IsMenuOpen("PipboyMenu")                    │
│  UI.Set("HUDMenu", "_root.health", 0.75)        │
│  UI.Invoke("BarterMenu", "_root.refresh", args) │
└──────────────┬──────────────────────────────────┘
               │ Native function bridge
┌──────────────▼──────────────────────────────────┐
│  UI Script Object (global functions)            │
│  IsMenuOpen, IsMenuRegistered                   │
│  OpenMenu, CloseMenu                            │
│  Get, Set, Invoke, Load                         │
│  RegisterCustomMenu, RegisterBasicCustomMenu    │
└──────────────┬──────────────────────────────────┘
               │ Scaleform GFx bridge
┌──────────────▼──────────────────────────────────┐
│  Scaleform GFx Runtime                          │
│  ActionScript 2.0/3.0 VM                        │
│  .swf files in Data/Interface/                  │
└──────────────┬──────────────────────────────────┘
               │ Render to texture
┌──────────────▼──────────────────────────────────┐
│  Engine Renderer (composited onto viewport)     │
└─────────────────────────────────────────────────┘
```

## Menu Registry (34 menus)

Every menu is identified by a string name used in `UI.IsMenuOpen()`,
`UI.OpenMenu()`, and `RegisterForMenuOpenCloseEvent()`.

### Gameplay Menus
| Menu | Purpose |
|---|---|
| `HUDMenu` | Main gameplay overlay (health, compass, crosshair) |
| `PipboyMenu` | Inventory, map, quests, radio, stats |
| `FavoritesMenu` | Quick-select favorites wheel |
| `VATSMenu` | Targeting system overlay |
| `DialogueMenu` | NPC conversation |
| `BarterMenu` | Trading with NPCs |
| `ContainerMenu` | Looting/transferring items |
| `ExamineMenu` | Item inspection |
| `BookMenu` | Reading books/notes |
| `TerminalMenu` | Computer terminal interaction |
| `TerminalHolotapeMenu` | Holotape games/programs |
| `LockpickingMenu` | Lock picking minigame |
| `ScopeMenu` | Weapon scope overlay |
| `WorkshopMenu` | Settlement building |
| `Workshop_CaravanMenu` | Supply line assignment |
| `CookingMenu` | Crafting at cooking stations |

### Character Menus
| Menu | Purpose |
|---|---|
| `LooksMenu` | Character appearance editor |
| `SPECIALMenu` | Initial SPECIAL stat allocation |
| `LevelUpMenu` | Perk selection on level up |

### System Menus
| Menu | Purpose |
|---|---|
| `MainMenu` | Title screen |
| `PauseMenu` | In-game pause/settings |
| `LoadingMenu` | Loading screens |
| `CreditsMenu` | Game credits |
| `MessageBoxMenu` | Modal message dialogs |
| `PromptMenu` | Confirmation prompts |
| `FaderMenu` | Screen fade transitions |
| `VignetteMenu` | Vignette overlay effect |
| `SitWaitMenu` | Waiting while sitting |
| `SleepWaitMenu` | Waiting while sleeping |
| `Console` | Debug console |
| `ConsoleNativeUIMenu` | Native UI console variant |
| `CursorMenu` | Cursor overlay |
| `GenericMenu` | Generic/custom menus |
| `MultiActivateMenu` | Multiple activation targets |

## Papyrus UI Script API

The `UI` script object provides the bridge between Papyrus and Scaleform:

```papyrus
; Query menu state
bool UI.IsMenuOpen(string menuName)
bool UI.IsMenuRegistered(string menuName)

; Open/close menus
UI.OpenMenu(string menuName)
UI.CloseMenu(string menuName)

; Read/write Flash properties by dot-path
var  UI.Get(string menuName, string path)
     UI.Set(string menuName, string path, var value)

; Call Flash functions
var  UI.Invoke(string menuName, string path, var[] args)

; Load external SWF into a menu
     UI.Load(string menuName, string path, string root)

; Register custom menus (for mod-created UI)
     UI.RegisterCustomMenu(string menuName, ...)
     UI.RegisterBasicCustomMenu(string menuName)
```

The `Get`/`Set`/`Invoke` pattern is essentially a property bridge using
ActionScript dot-path notation (e.g., `_root.HUDObject.health`).

## Menu Events

Scripts can listen for menu state changes via ScriptObject:

```papyrus
RegisterForMenuOpenCloseEvent(string menuName)
UnregisterForMenuOpenCloseEvent(string menuName)
UnregisterForAllMenuOpenCloseEvents()

Event OnMenuOpenCloseEvent(string menuName, bool opening)
```

Additional events: `OnLooksMenuEvent` (character appearance changes),
`OnTutorialEvent` (tutorial triggers).

## InputEnableLayer System

Controls which input is active during different game states. Scripts create
layers that enable/disable categories:

```papyrus
InputEnableLayer layer = InputEnableLayer.Create()
layer.EnableMenu(true)
layer.EnableFavorites(false)
```

Categories: movement, looking, activation, fighting, sneaking, menu, journal,
VATS, favorites, running, camera switch, fast travel, jumping.

Layered design means multiple scripts can manage input without conflicts — each
layer is independent and the system ANDs them together.

## Text Replacement

Two substitution systems for displayed strings:

- **Text Replacement** — dynamic token substitution in displayed text
  (e.g., `<Alias=Player>` replaced with player name)
- **Button Tag Replacement** — maps button references to platform-specific
  icons (keyboard vs gamepad)

## Technology Stack

- **Scaleform GFx** — proprietary Flash runtime (discontinued)
- **SWF files** in `Data/Interface/` directory
- **ActionScript** (2.0 and 3.0) for menu logic
- **Tooling:** Adobe Flash (authoring), Adobe Illustrator (assets),
  FFDec / JPEXS Free Flash Decompiler (reverse engineering)

---

## ByroRedux Strategy

### Native UI Framework

New ByroRedux content will use a modern Rust-native UI framework (TBD —
candidates include `egui`, `iced`, or a custom immediate-mode system built
on our Vulkan renderer). The UI system will be data-driven with menus
defined as ECS components.

### Legacy SWF Compatibility: Ruffle

For loading original Skyrim/Fallout 4 `.swf` menu files from legacy content:

**[Ruffle](https://ruffle.rs/)** — a Rust-native Flash emulator. Key advantages:
- Written in Rust — can integrate as a crate, no FFI gymnastics
- Active open-source project with broad Flash compatibility
- Renders to a texture that we composite onto our Vulkan viewport
- Supports ActionScript 2.0 (good) and progressively AS3 (improving)

**Integration architecture:**
```
Legacy .swf files
       │
       ▼
Ruffle (Rust crate)
       │ renders to pixel buffer
       ▼
Vulkan texture upload (staging → device-local)
       │
       ▼
UI composition pass (overlay onto viewport)
```

**Bridge requirements:**
- Menu string identifiers must be preserved — they're hardcoded throughout
  legacy scripts (`"PipboyMenu"`, `"HUDMenu"`, etc.)
- `UI.Get/Set/Invoke` calls must be translated to Ruffle's ActionScript VM
- `OnMenuOpenCloseEvent` must fire when Ruffle menus open/close
- InputEnableLayer system must integrate with Ruffle's input handling

### ECS Mapping

| UI Concept | ECS Equivalent |
|---|---|
| Menu string names | Component field on MenuState entity |
| `UI.Get/Set` property bridge | Component field access on menu entity |
| `UI.Invoke` function calls | System dispatches to menu entity |
| `InputEnableLayer` | Layered input resource (AND-combined layers) |
| Menu open/close events | State watch on MenuState component |
| Text replacement | System that processes string templates |
| Custom menus | Mod-registered menu components |

---

## References

- [User Interface Category](https://falloutck.uesp.net/wiki/Category:User_Interface)
- [Menu](https://falloutck.uesp.net/wiki/Menu)
- [UI Script](https://falloutck.uesp.net/wiki/UI_Script)
- [Flash File](https://falloutck.uesp.net/wiki/Flash_File)
- [Actionscript Reference](https://falloutck.uesp.net/wiki/Actionscript_Reference)
- [Text Replacement](https://falloutck.uesp.net/wiki/Text_Replacement)
- [Ruffle Project](https://ruffle.rs/)
- Internal: `docs/engine/scripting.md` (scripting architecture)
