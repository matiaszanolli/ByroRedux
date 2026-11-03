# Legacy Mod Compatibility — Product Requirements

**Status**: ACTIVE (2026-11-03). Revision 5 files the tracking issues.
Revision 4 recorded the owner's §10 decisions; every one is decided. Q1
changed the promise for unofficial patches: a known patch is not loaded,
the engine stands in for it (LMC-FIX-004), and the user is told it is not
needed. Revision 3 (2026-11-01) folded in two review rounds
(loose-root precedence and ownership, ITM and list-merge semantics, missing
masters, level-aware per-mod version probes, archive pairing, new catalog
rows, save semantics, Starfield medium plugins). Statements about how the
original engines behave cite external sources (UESP and the Creation Kit /
GECK wikis, extender sources, tool documentation); a claim that could not
be verified is marked **Phase 0: verify** instead of being stated as fact.

**Tracking**: #6664 (parent). Phases: #6669 (0), #6670 (1), #6671 (2), #6672 (3), #6673 (4), #6674 (5), #6675 (6), #6676 (7).

**Scope**: What it means for a mod authored for Oblivion, Fallout 3, Fallout:
New Vegas, Skyrim (LE / SE / AE), Fallout 4 or Starfield to "work" on
ByroRedux; which mod mechanisms are in scope; what the engine promises for
each; and how that promise is measured. Fallout 76 is out of scope (no
official mod support, no corpus).

**Related**: this document is the umbrella over designs that already own
the mechanisms. It states the *promise*; they own the *how*:

| Mechanism | Owning design |
|---|---|
| Fix parity (unofficial patches, engine-fix plugins) | [Built-in Fixes](../engine/builtin-fixes.md) |
| Executable mods, sandbox, SDK, extender API facade | [Sandboxed Linked Mods](../engine/sandboxed-linked-mods.md), [SDK v0.1 plan](../engine/sdk-v0.1-development-plan.md), [SDK next actions](../engine/sdk-v0.1-next-action-plan.md) |
| Record plugins, Form IDs, conflict resolution | [Plugin Loading](../engine/plugin-loading.md), [ESM Records](../engine/esm-records.md) |
| Archives, loose files, asset precedence | [Archives](../engine/archives.md), [Asset Pipeline](../engine/asset-pipeline.md) |
| Papyrus / ObScript execution | [Scripting](../engine/scripting.md), [M47.2 design](../engine/m47-2-design.md) |
| Scaleform / MenuXml UI | [UI System](../engine/ui.md) |
| Saves | [Save/Load Round-Trip](../engine/save-load-roundtrip.md) |
| Front end, profiles, install detection | [Launcher](../engine/launcher.md) |
| Per-game engine tiers | [Game Compatibility](../engine/game-compatibility.md) |

Normative terms **MUST**, **SHOULD** and **MAY** have their RFC 2119
meanings, as in the related designs. Requirement IDs use the `LMC-` prefix.

---

## 1. Product thesis

Legacy mod compatibility is the engine's key feature and its hardest one.
The mod ecosystem is the content library ByroRedux exists to run. Three
statements, given by the project owner, define the product:

1. **Right the wrongs.** The original engines' limits are defects, not
   features to preserve: load-slot ceilings, load-order-bound identity, the
   "rule of one" record override, a lagging script VM that serialises its
   stacks into saves, frame-rate-bound physics, precombine fragility, 32-bit
   memory. The engine removes them (§6).
2. **Reasonable compatibility.** Mods that are *content* (records, assets,
   scripts, UI, settings) work as authored. Mods that are *workarounds* for
   engine defects are neither needed nor pretended to work. The engine says
   which is which, per mechanism, before launch.
3. **Fixes are built in; patches and extenders are not required.** A known
   unofficial patch is not loaded: the engine already carries its fixes,
   stands in for it towards the mods that depend on it, and tells the user
   it is not needed (decision Q1, LMC-FIX-004).
   Extender-dependent mods run without the extender because the engine owns
   the script-facing surface those extenders added. The extender binary
   itself is never loaded ([NG-001](../engine/sandboxed-linked-mods.md#5-non-goals)).

The corollary that shapes everything below: **compatibility is promised per
mechanism, never as a global flag**
([API-009](../engine/sandboxed-linked-mods.md#15-api-evolution-and-compatibility)).
"Does SkyUI work?" has no single answer; "do SkyUI's `.swf` menus, its
Papyrus scripts, and its SKSE plugin work?" has three, and the engine
reports all three.

---

## 2. Terminology

| Term | Meaning |
|---|---|
| **Legacy mod** | Anything distributed for one of the target games: plugins, archives, loose files, scripts, UI movies, native plugins, INI files, generated patches. |
| **Mechanism** | One way a mod attaches to the game: "record override", "loose texture", "SKSE script function", "Scaleform menu", "native DLL hook". The unit of the compatibility promise. One mod uses several. |
| **Disposition** | The engine's stance on a mechanism, one of the five classes in §3. |
| **Mod profile** | The user's enabled set: legacy plugins in order, archives, loose directories, extension packages, grants, policies. The sandbox design's *mod profile* ([§3](../engine/sandboxed-linked-mods.md#3-terminology)). The only input to activation. |
| **Profile order** | The user's ordering of legacy plugins inside the profile. Replaces `plugins.txt`. |
| **Mod root** | One mod's loose-file tree, as a mod manager would deploy it, together with the plugins and archives it ships (LMC-ID-012). The profile orders roots independently of plugins (LMC-AST-010). |
| **Legacy slot** | The load-order prefix of a legacy Form ID: 8-bit for full plugins, `0xFE`+12-bit for small/light plugins, and `0xFD`+8-bit over a 16-bit object ID for Starfield medium plugins (R9, LMC-ID-005, LMC-ID-008). Decode-only in Redux ([Plugin Loading](../engine/plugin-loading.md#form-id-system)). |
| **Fix stack** | The unofficial patch plus the extender and fix plugins a game needed to run properly ([Built-in Fixes §3](../engine/builtin-fixes.md#3-fix-catalog)). |
| **Redundant** | A mod, or part of one, whose only purpose Redux already serves. Loads harmlessly if it is data; is never loaded if it is code; is always reported. |
| **Conformance corpus** | The set of real mods, per game, that the compatibility claim is measured against (§7). Local, opt-in, never checked in. |

---

## 3. The disposition model

Every mechanism, per game, has exactly one disposition:

| Class | Name | Meaning | Reported as |
|---|---|---|---|
| **D1** | Works | A supported engine input. The mod behaves as authored. Gaps inside the mechanism (an unmodelled script call, an unhandled host method) are attributed diagnostics, never silence. | "active" |
| **D2** | Redundant | Exists to fix or work around an engine defect Redux does not have. Data parts load and may win by load order; code parts are not loaded. Never an error. | "redundant: replaced by <catalog entries>" |
| **D3** | Not applicable | Targets the original executable, its renderer or its save format. There is nothing to port. | "not applicable: <reason>" |
| **D4** | Port | Valid gameplay semantics behind a native-code mechanism. Supported by source port to the SDK. Where many mods depend on a library's *script-facing* API, the engine owns that API as a compatibility service, so dependents run without the port. | "requires port: <guide>" or "served by <service>" |
| **D5** | Unsupported, tracked | Recognised, no route yet, catalogued with a reason. | "unsupported: <reason>, tracked <id>" |

- **LMC-CLS-001.** Every mechanism in the catalog (§3.1) MUST have exactly
  one disposition per target game, with a cited reason.
- **LMC-CLS-002.** Dispositions are per mechanism. A mod that spans several
  MUST be reported per mechanism, never collapsed to one verdict.
- **LMC-CLS-003.** Recognising a mechanism MUST NOT be reported as supporting
  it. This is the SDK plan's rule ("recognising the provider is not enough
  to claim compatibility") applied to every layer.
- **LMC-CLS-004.** D2 and D3 MUST NOT stop profile activation. D5 MUST NOT
  stop it either unless the mechanism is a required dependency of something
  the user enabled, in which case the dependent is reported
  ([LINK-013](../engine/sandboxed-linked-mods.md#92-link-resolution)). A legacy
  plugin whose master is absent is the record-layer case of the same rule
  and is handled by LMC-ID-011: the plugin and its dependents are disabled
  and reported, and activation continues.
- **LMC-CLS-005.** A disposition change MUST be versioned and cited, the
  same way a fix catalog entry is ([FIX-003](../engine/builtin-fixes.md#1-requirements)).
- **LMC-CLS-006.** The catalog MUST be machine-readable and MUST drive the
  preflight report (§5.11); prose tables are derived from it, not the other
  way round.

### 3.1 Mechanism catalog (starting inventory)

The disposition column is the *target*. The last column is the state at
HEAD on 2026-11-01, from the live code and docs cited; "verify" means the
behaviour was not confirmed for this draft and is Phase 0 work. Mod names
are examples of the mechanism, not a compatibility claim about those mods.

The "Today" column is a hand-written starting inventory and will go stale.
Once Phase 0 lands the machine-readable catalog (LMC-CLS-006), this column
is either generated from it or removed; it MUST NOT be maintained by hand
after that point.

**Content: records**

| # | Mechanism | Examples | Disp. | Today |
|---|---|---|---|---|
| R1 | New records in a plugin (ESM/ESP/ESL/ESH) | every content mod | D1 | Multi-plugin fold in `cell_loader/load_order.rs` (M46.0); full-profile loading is M46, not shipped |
| R2 | Record overrides of a master's records | patches, overhauls | D1 | Last-in-order wins at the fold; deletions carried (`EsmIndex::merge_from`) |
| R3 | Deleted / disabled records (DELE flag, UDR tombstones) | cleaned plugins, cell edits | D1 | Tombstones decoded (`FLAG_DELETED`, `deleted_records`); REFR semantics: verify |
| R4 | Injected records (a plugin defines a form in a master's ID space) | AWKCR-style, Bashed Patch | D1 | verify |
| R5 | Cell / worldspace overrides, new worldspaces, REFR edits | new lands, cell overhauls | D1 | Cell walker per plugin; override merge: verify per field |
| R6 | Navmesh overrides | any cell edit | D1 (data) | NAVM decoded; runtime pathfinding PROPOSED ([navmesh-pathfinding.md](../engine/navmesh-pathfinding.md)) |
| R7 | Localised string tables (`STRINGS` / `DLSTRINGS` / `ILSTRINGS`) | Skyrim+ localised plugins | D1 | `esm/strings_table.rs` |
| R8 | Plugin header / form versions beyond the vanilla build | SE 1.71-header plugins, FO4 next-gen | D1 | Parser accepts per game; "backported header support" plugins → D2 |
| R9 | Small / light flag (`FExxxyyy`, 4,095 records per plugin) and Starfield medium flag (`FDxxyyyy`, 65,535 records per plugin) ([Ortham, Load order in Starfield](https://blog.ortham.net/posts/2024-06-28-load-order-in-starfield/)) | ESPFE, Creation Club, Starfield medium plugins | D1 | `0xFE` decoded; `0xFD` not modelled; both spaces are covered by the virtual index (LMC-ID-005) |
| R10 | GMST overrides from plugins | tweak mods | D1 | Consumers that hard-code authored GMSTs break this class (#6567 is one instance) |
| R11 | SEQ files (`seq/<plugin>.seq`) listing Start Game Enabled quests, which the original engine needs "in order to track Start-Game Enabled quests you've added" so their dialogue and scenes work ([CK: Quest Data Tab](https://ck.uesp.net/wiki/Quest_Data_Tab), [Seq Guide](https://wiki.beyondskyrim.org/wiki/Arcane_University:Seq_Guide)) | any Skyrim+ quest mod | D2 | The engine starts such quests from the QUST record itself (LMC-SAV-003); the file is never read. Phase 0: confirm every load path starts them without it |

**Content: assets**

| # | Mechanism | Examples | Disp. | Today |
|---|---|---|---|---|
| A1 | Loose files under `Data\` shadowing archives | texture packs, mesh fixes | D1 | Shipped: loose tier outranks every archive (#6200) |
| A2 | Plugin-paired archives (per-game naming rules in LMC-AST-002) | most archived mods | D1 | **Not implemented**: archives come from explicit `--bsa` flags and profile defaults; only `<stem>N` numeric siblings auto-load (#784). Naming verified for Oblivion, FO3/FNV exact-name, FO4 and Starfield main/textures; the rest are Phase 0 items listed under LMC-AST-002 |
| A3 | Archive list order (`sResourceArchiveList`, `sArchiveList`, `.ccc`) | official + CC content | D1 (order semantics) | Profile-driven lists in `GameProfileEntry`; no INI or `.ccc` reader exists |
| A4 | Archive invalidation (`bInvalidateOlderFiles`, redirection archives such as `ArchiveInvalidationInvalidated!.bsa`): how Oblivion and FO3/FNV are made "to recognize certain new files placed in the /Data/ folder" ([GECK: BSA Files](https://geck.uesp.net/wiki/BSA_Files), [UESP: BSAs and Archive Invalidation](https://en.uesp.net/wiki/Oblivion_Mod:BSAs_and_Archive_Invalidation)) | FO3/FNV/Oblivion texture mods | D2 | Loose files always win in this engine (A1), so the flag and the redirection archive are ignored with a note |
| A5 | DDS textures from mod tooling | all texture mods | D1 | Formats in `crates/renderer/src/vulkan/dds.rs` (BC1/3/5/7, RGBA8, BGRA seen); mod corpus must gate the rest |
| A6 | NIF meshes from mod tooling (NifSkope, Blender, 3ds Max exporters) | all mesh mods | D1 | Parser corpus is vanilla-only today; mod NIFs join the corpus |
| A7 | Sound / voice / lip (`.wav`, `.xwm`, `.mp3`, `.fuz`, `.lip`) | voiced mods | D1 | M44 audio; `.fuz` header bug open (#6480) |
| A8 | SpeedTree `.spt` replacements | Oblivion/FO3/FNV tree mods | D1 | `crates/spt` + billboard import |
| A9 | FaceGen data (`facegendata/`) overrides | NPC overhauls | D1 | M41 FaceGen; verify override by loose path |
| A10 | LOD-generator outputs: DynDOLOD's `DynDOLOD.esp`, conditional `DynDOLOD.esm` and `Occlusion.esp`, object / tree / grass LOD meshes and textures, TexGen textures and billboards, xLODGen terrain LOD ([Generation Instructions](https://dyndolod.info/Help/Generation-Instructions)); DynDOLOD DLL, "a SKSE plugin and its accompanying papyrus scripts" ([DynDOLOD DLL](https://dyndolod.info/Help/DynDOLOD-DLL)) | DynDOLOD, TexGen, xLODGen | D1 for the generated plugins and static LOD assets; D2 for `Occlusion.esp` and the DLL (the engine has no occlusion planes to feed and owns dynamic LOD) | The `.bto` / `.btr` ladder (M35) consumes generated static LOD like vanilla LOD. Position: consume as assets until engine-side LOD generation (M53) makes them redundant; the DLL is never loaded |
| A11 | Grass caches: `Data\Grass\` files generated by No Grass In Objects per its `GrassControl.ini` ([NGIO NG](https://github.com/DwemerEngineer/No-Grass-In-Objects-NG)); the `.cgid` extension and the vanilla `.gid` cache are community-reported | NGIO, Grass Cache Helper NG | D3 (cache and the generating SKSE plugin) | The engine does not run the original grass generator; its ground cover is engine-authored ([exal-groundcover.md](../engine/exal-groundcover.md)), so there is nothing for a precomputed cache to feed. Grass *LOD* billboards from A10 remain assets |

**Content: scripts**

| # | Mechanism | Examples | Disp. | Today |
|---|---|---|---|---|
| S1 | Papyrus `.pex` (new scripts, overriding vanilla scripts) | Skyrim+/FO4/SF mods | D1 | Lowering + bounded VM (M47.2); per-game rates in ROADMAP |
| S2 | ObScript in `SCPT` records | Oblivion/FO3/FNV mods | D1 | SCDA interpreter (M47.3) |
| S3 | Extender *language* extensions in ObScript (NVSE/OBSE `let`/`eval`, arrays, string vars, user-defined functions) | JIP-era FNV mods, OBSE mods | D5 → D1 | Not modelled; version probes and load-order commands only |
| S4 | Extender *script functions* (SKSE/F4SE/xNVSE/JIP/OBSE calls visible from Papyrus/ObScript) | nearly every scripted mod post-2012 | D1 per function | Catalog in `crates/sdk/src/compatibility/` (SKSE `Game.*` ×11, xNVSE/OBSE load-order ×5, Input ×2, UI ×1); rest D5 |
| S5 | Script-library DLLs' Papyrus API (PapyrusUtil `StorageUtil`, JContainers, powerofthree's Papyrus Extender, ConsoleUtil, UIExtensions) | frameworks | D4, served by engine services | `StorageUtil` global scalars/lists, JContainers core, ModEvent shipped; file-backed/object-scoped/JDB unsupported |
| S6 | NVSE script runner `.txt` scripts, OBSE plugin INIs | FNV/Oblivion tweak mods | D5 | Not modelled |
| S7 | VMAD / quest fragments / scene fragments from mods | quest mods | D1 | Same attach path as vanilla |
| S8 | Papyrus VM tuning INIs (`fPostLoadUpdateTimeMS`, `iMaxAllocatedMemoryBytes`, …) | "performance" guides | D2 | No VM to tune |

**Native code**

| # | Mechanism | Examples | Disp. | Today |
|---|---|---|---|---|
| N1 | Extender runtime binaries (SKSE/F4SE/xNVSE/OBSE/SFSE) | — | D2 (the facilities) + D3 (the binary) | Never loaded; facilities are SDK services |
| N2 | Engine-fix plugins | SSE Engine Fixes, Bug Fixes SSE, Scrambled Bugs, NVAC, NVTF, Buffout 4, Oblivion engine fixes | D2 | Each fix re-derived into the [fix catalog](../engine/builtin-fixes.md#3-fix-catalog); catalog Phase 0 not started |
| N3 | Memory / heap / LAA / crash-logger / address-library / preloader plugins | 4GB patch, SSE Display Tweaks' allocator half, Address Library | D3 | 64-bit Rust; nothing to port |
| N4 | Gameplay plugins with engine hooks | Precision, TK Dodge RE, Place Everywhere, Better Jumping | D4 | SDK port; porting guide ([CPP-008](../engine/sandboxed-linked-mods.md#16-c-and-c-sdk-requirements)) not written |
| N5 | Renderer injectors | ENB, ReShade, Community Shaders, Oblivion Reloaded | D3 | Different renderer by design |
| N6 | Behaviour-graph generators' *outputs* (FNIS/Nemesis/Pandora: `behaviors/*.hkx`, `animations/*.hkx`, `animationdatasinglefile.txt`) | animation frameworks | D1 (assets) | `crates/hkx` reads Skyrim/FO4 graphs; modded-graph runtime unverified; ambient locomotion gap (feature-matrix) |
| N7 | Conditional animation replacement conventions (DAR `_CustomConditions`, OAR `config.json`) | animation mods | D4 → engine-native | Not built; see §6 row 18 |
| N8 | Head / face asset overrides needing a DLL (Blockhead for Oblivion) | OCOv2 | D4 → engine-native | Not built |
| N9 | Character-editor plugins (RaceMenu, LooksMenu) and body morph runtime (`.tri`) | body/face mods | D5 | Runtime morph updates ✗ (feature-matrix) |

**UI**

| # | Mechanism | Examples | Disp. | Today |
|---|---|---|---|---|
| U1 | Scaleform menu replacement `.swf` | SkyUI, DEF_UI, HUDFramework, StarUI | D1 within the host-method catalog | Skyrim/SkyUI 142-method and FO4 `BGSCodeObj` 269-method catalogs (M48); unmapped methods are diagnostics |
| U2 | Mod Configuration Menus (SkyUI MCM, MCM Helper, FO4 MCM, FNV MCM via UIO) | most configurable mods | D4 → engine-owned MCM host | `UI.IsMenuOpen` only; registration and mutation unsupported |
| U3 | MenuXml menu replacements and prefabs | DarNified UI, Vanilla UI Plus, oHUD, UIO | D1 | `crates/menuxml` (M48.4/M48.5); `<include>` prefabs supported |
| U4 | Translation files (`Interface/Translations/*_<lang>.txt`) | SkyUI, localised mods | D1 | verify |
| U5 | Font replacements (`fontconfig.txt`, `.swf` fonts, `.fnt`) | UI mods | D1 | verify |

**Settings and tooling outputs**

| # | Mechanism | Examples | Disp. | Today |
|---|---|---|---|---|
| T1 | Unofficial patches as plugins | UOP, UFO3P, YUP, USSEP, UFO4P, SCP | D2, with a stand-in for dependents (LMC-FIX-004, decision Q1) | Loads as any plugin; fix catalog pending |
| T2 | Generated patch plugins | Bashed Patch, Smashed Patch, Synthesis output, Reqtificator output | D1 (plugin) + D2 (necessity where §5.2 merge covers it) | Loads as any plugin |
| T3 | Merged plugins, ESL-flagged plugins, "cleaned" masters | load-order management | D1 (plugin) + D2 (necessity) | No slot ceiling in the design; see LMC-ID-004 |
| T4 | FO4 precombine / previs repair packs | PRP | D1 (data) + D2 (necessity) | REFR fallback when precombines are absent (M49); forced REFR-first on override: see LMC-REC-012 |
| T5 | Game INI edits for removed limits (`uGridsToLoad`, heap, `bInvalidateOlderFiles`, Papyrus) | performance guides | D2 | INI not read |
| T6 | Game INI edits for graphics | ENB guides, `Ultra.ini` tweaks | D3 | Engine settings registry owns graphics (`core/settings`) |
| T7 | Game INI edits for gameplay-visible values | FOV, difficulty, `fXP…` | D1 where the setting exists in the registry; D5 otherwise | Honoured INI keys: none today; import table to be defined (LMC-SET-001) |
| T8 | Mod-provided INIs / JSON (`Data/SKSE/Plugins/*.ini`, `MCM/Settings/*.json`, `NVSE/Plugins/*.ini`) | configurable DLL mods | D4 | Readable by a ported extension through scoped content read ([SEC-018](../engine/sandboxed-linked-mods.md#102-requirements)) |
| T9 | Cosaves (`.skse`, `.f4se`, `.nvse`) and save editors | ReSaver, FallrimTools | D3 | No legacy save format (ROADMAP anti-scope) |
| T10 | Mod-manager virtual filesystems (MO2 USVFS) and deployment layouts (Vortex) | — | D3 (VFS) + D1 (their outputs, via profile import) | No `plugins.txt` / `modlist.txt` reader exists |
| T11 | "Plugin limit" and `plugins.txt` enablers, achievement enablers | Starfield early mods | D2 / D3 | — |
| T12 | Total conversions | Tale of Two Wastelands, Enderal | D1 (content) | Not attempted; corpus tier decision Q6 |

---

## 4. Goals and non-goals

### 4.1 Goals

- **LMC-G-001 — Content works as authored.** For each game at engine Tier 1
  ([game-compatibility.md](../engine/game-compatibility.md#per-game-support-detail)),
  every D1 mechanism in §3.1 is exercised by the conformance corpus and
  passes its acceptance scenario (§7.3).
- **LMC-G-002 — No required fix stack.** Vanilla data on ByroRedux behaves as
  if the game's fix stack were installed ([FIX-001](../engine/builtin-fixes.md#1-requirements)).
  A user who installs the stack anyway is told it is not needed; a known
  unofficial patch is not loaded (LMC-FIX-001), the extender binary never is
  (LMC-EXT-001), and mods that depend on either still run (LMC-FIX-004,
  LMC-G-003).
- **LMC-G-003 — Extender-free.** A mod that depends on an extender's
  script-facing API runs without the extender when its calls are in the
  catalog. Coverage is stated per function, per family, per game.
- **LMC-G-004 — Identity without order.** No slot ceiling, no sorter, no
  Form ID renumbering, saves independent of profile order
  ([ARC-008](../engine/sandboxed-linked-mods.md#71-package-content-and-attachment),
  [SCALE-001](../engine/sandboxed-linked-mods.md#14-performance-and-scale)).
- **LMC-G-005 — Explainable before launch.** A profile can be checked
  without starting the renderer, and the check names every mechanism of
  every mod with its disposition and every conflict with its winner.
- **LMC-G-006 — Profile changes are safe.** Adding, removing or reordering
  mods against an existing save has defined, reported semantics (§5.10).
  "Never uninstall a mod mid-playthrough" is a legacy rule, not ours.
- **LMC-G-007 — Measured.** Compatibility is a published number per game,
  derived from the corpus the same way parse rates are, with one home
  (ROADMAP's compatibility matrix).
- **LMC-G-008 — Linux-first parity.** Case-insensitive names and paths, path
  separators and archive keys behave as the games' own Windows filesystem
  did, so a mod packaged for Windows needs no renaming.

### 4.2 Non-goals

- **LMC-NG-001.** Loading native extender or plugin binaries, or reproducing
  their ABI ([NG-001](../engine/sandboxed-linked-mods.md#5-non-goals),
  [NG-002](../engine/sandboxed-linked-mods.md#5-non-goals)).
- **LMC-NG-002.** Renderer injectors (ENB, ReShade, shader replacers). The
  renderer is not the original and matching its look is anti-scope.
- **LMC-NG-003.** Legacy save files and cosaves (ROADMAP anti-scope).
- **LMC-NG-004.** A load-order sorter or metadata masterlist (ROADMAP
  anti-scope, "no LOOT-equivalent"). Masters-first is a constraint the
  engine enforces; everything else is the user's order.
- **LMC-NG-005.** Redistributing any mod, patch or fix content
  ([NG-007](../engine/sandboxed-linked-mods.md#5-non-goals),
  [FIX-002](../engine/builtin-fixes.md#1-requirements)).
- **LMC-NG-006.** Emulating a mod manager's virtual filesystem. Their outputs
  are imported; their runtime is not hosted.
- **LMC-NG-007.** Bethesda.net / Creation Club login, purchase or download
  flows. Already-installed CC content is ordinary archived content.
- **LMC-NG-008.** Fallout 76.
- **LMC-NG-009.** Bug-for-bug reproduction of engine behaviour that mods
  accidentally depend on, unless a catalogued fix entry records the
  decision to keep it ([builtin-fixes.md §2.1](../engine/builtin-fixes.md#21-engine-behaviour-absent-by-construction-is-a-claim-not-an-assumption),
  GAME-005).

---

## 5. Requirements by layer

### 5.1 Identity, profile and order (`LMC-ID`)

- **LMC-ID-001.** A legacy plugin's stable identity MUST be derived from its
  case-folded basename. Today `PluginId::from_filename` hashes the name as
  given while the load-order key folds to lowercase
  (`plugin_basename_lc`); the two MUST agree, and `Skyrim.esm` and
  `skyrim.esm` MUST be one plugin.
- **LMC-ID-002.** The mod profile is the only input to activation. The engine
  MUST NOT read `plugins.txt`, `loadorder.txt`, `.ccc` or game INIs at
  runtime; those are *import* sources (LMC-ID-006).
- **LMC-ID-003.** Among plugins with no dependency relation, profile order
  decides overrides. A plugin MUST load after every master it declares; the
  engine enforces that constraint by hoisting masters and reports the
  reordering. No other reordering is performed (LMC-NG-004). The resolver's
  current UUID tie-break ([plugin-loading.md](../engine/plugin-loading.md#dependencyresolver--conflict-resolution))
  MUST be replaced by profile order for legacy content.
- **LMC-ID-004.** There MUST be no ceiling on regular, light or medium
  plugins in a profile. Legacy slot bytes are decoded, never allocated as a
  limit. A profile with more than 254 regular plugins MUST load.
- **LMC-ID-005 — Virtual legacy index.** Compatibility calls that return a
  load-order index (`GetModByName`, `GetModIndex`, `GetNthModName`,
  `GetFormFromFile` with a numeric prefix) MUST be served from a
  per-session virtual index that numbers plugins exactly as the game would
  have: profile order for the first 254 regular plugins, the `FExxxyyy`
  small/light space, and for Starfield the `FDxxyyyy` medium space, whose
  index counts active medium plugins ("the first active medium plugin is
  given `00`, the second `01`", [Ortham](https://blog.ortham.net/posts/2024-06-28-load-order-in-starfield/)).
  Beyond those ranges a documented sentinel is returned per plugin. Today a
  catalog that cannot be represented in 8 bits is rejected as a whole
  (`crates/sdk/src/compatibility/game_content.rs`); it MUST degrade per
  plugin instead. Script literals of runtime Form IDs (`0x05001234`) resolve
  through the same virtual index.
- **LMC-ID-006 — Import.** The launcher MUST import a legacy load order from
  `plugins.txt` / `loadorder.txt`, a Mod Organizer 2 profile
  (`modlist.txt` + `plugins.txt`) and a Vortex deployment into a mod
  profile, one shot, with a diagnostic per entry it cannot resolve. The
  import populates both orders the profile carries: plugin order from
  `plugins.txt`, and mod-root order (LMC-AST-010) from `modlist.txt`
  priority or Vortex's rules. An MO2 profile with "Have MO manage
  archives" on ranks archives by mod order rather than plugin order
  ([STEP: Mod Organizer guide](https://stepmodifications.org/wiki/Guide:Mod_Organizer));
  the import reports that the engine does not reproduce that mode. The
  import also records ownership (LMC-ID-012): each MO2 mod directory and
  each Vortex-deployed mod becomes a root that owns the plugins, archives
  and loose files it contains. Launcher P4+ work
  ([launcher.md](../engine/launcher.md)).
- **LMC-ID-007.** A profile MUST carry a fingerprint over plugin identities,
  order, content hashes, extension packages, grants and policies
  ([PKG-010](../engine/sandboxed-linked-mods.md#8-package-manifest-requirements),
  [STATE-011](../engine/sandboxed-linked-mods.md#13-persistent-state-and-saves)).
  Saves store it (§5.10). Today a save stores only the cell, ESM path and
  master list (`CurrentCellContext`).
- **LMC-ID-008.** ESL / ESH / ESM header flags affect only Form ID decoding
  and masters-first ordering. "Small" MUST NOT impose a form count beyond
  the 12-bit object space the file was authored in (4,095 records), and
  "medium" MUST NOT impose one beyond its 16-bit object space (65,535
  records) ([Ortham](https://blog.ortham.net/posts/2024-06-28-load-order-in-starfield/)).
  The original engines' active-plugin ceilings for each class do not exist
  here (LMC-ID-004).
- **LMC-ID-009.** Plugin names, archive names, asset paths and script names
  MUST resolve case-insensitively, with `\` and `/` equivalent, on every
  host filesystem.
- **LMC-ID-010.** Two enabled packages with the same identity and different
  content MUST be an activation diagnostic, never a silent replacement
  ([PKG-013](../engine/sandboxed-linked-mods.md#8-package-manifest-requirements)).
- **LMC-ID-011 — Missing masters.** When an enabled plugin declares a master
  that is not in the profile (a skipped unofficial patch counts as present,
  LMC-FIX-004), activation MUST disable that plugin and every
  plugin that transitively depends on it, report the chain (plugin, missing
  master, each disabled dependent), and continue activating the rest of the
  profile. The root that ships a disabled plugin (LMC-ID-012) is disabled
  with it, loose files and paired archives included, when every plugin the
  root ships is now disabled: its scripts and assets usually reference the
  missing forms. A root that still has a loadable plugin keeps its assets
  and loses only the affected plugins, because mods commonly ship optional
  compatibility patches as extra plugins whose master is another mod the
  user may not have; the report then warns that the root's assets may
  reference the disabled plugin's forms. A root with no plugins is never
  affected. The original engines crash instead: on Oblivion a plugin whose
  dependency is "not present at all … will definitely crash" the game
  before the main menu
  ([UESP](https://en.uesp.net/wiki/Oblivion_Mod:PCS_Library/CTD_on_load_(before_main_menu))).
  **Phase 0: verify** the equivalent behaviour on Skyrim SE, FO4 and
  Starfield from a primary source; community reports describe the same
  startup crash. A disabled dependent is reported under LMC-CLS-004 for
  that session, never silently omitted.
- **LMC-ID-012 — Ownership.** The profile MUST record, for every mod root,
  its package identity and the plugins, archives and loose files it ships;
  content found directly in the game's `Data` directory belongs to an
  implicit "unmanaged" root that sorts first. For a root without a
  manifest, the import derives the identity from the root's directory name,
  case-folded. Ownership is what lets LMC-ID-011 disable a whole mod,
  LMC-DX-005 attribute a diagnostic to the root whose file actually won,
  and LMC-SCR-010 answer probes per mod from the scripts that actually run.
  LMC-ID-006 populates it on import.

### 5.2 Records (`LMC-REC`)

- **LMC-REC-001.** Every record type an engine system consumes MUST be
  decoded from every plugin in the profile, not only from masters or the
  profile's default content. A consumer that reads a value only from the
  main ESM, or hard-codes an authored value (#6567), is a compatibility bug.
- **LMC-REC-002 — Baseline semantics.** The default override rule for a
  record is the legacy one: the last plugin in profile order that carries
  the record wins, as a whole. Mod authors document their mods against this
  rule and the engine MUST honour it so that documented load-order advice
  stays true. That promise holds in full only under the `legacy` policy;
  under `merge` (LMC-REC-003, decision Q2) list-valued sub-records are
  combined, and "load X after Y" then describes which scalar fields win,
  not which list entries survive.
- **LMC-REC-003 — List merge (right-the-wrong, decision Q2).** For
  list-valued sub-records (levelled-list entries, form lists, keyword lists,
  container and actor inventories, faction memberships, outfit and
  spell lists), the engine SHOULD offer a profile policy `merge`:
  - *Scope.* Only *independent* overrides merge (neither plugin declares the
    other as a master). A *dependent* override (a patch) wins wholesale
    over the plugins it depends on, as under `legacy`.
  - *Entries are multisets.* Levelled lists carry the same form more than
    once on purpose, at different levels or as exact duplicates, to shape
    the odds ("Deer are listed at level 1, 8, and 17"; elven weapons
    "appear twice in this list", [UESP: Oblivion Leveled Lists](https://en.uesp.net/wiki/Oblivion:Leveled_Lists)),
    and each entry carries its own level and count
    ([CK: LeveledItem](https://ck.uesp.net/wiki/LeveledItem)). A plugin's
    entries for one form are applied as the multiset the plugin authored;
    nothing is deduplicated. **Phase 0: verify** on Skyrim+ that duplicate
    entries weight the roll the way they do on Oblivion.
  - *Operations are per form.* Each override is diffed against the record
    it overrides form by form, comparing the multiset of (level, count,
    extra data) entries the plugin holds for that form with the master's:
    equal is *kept*; present only in the master is a *removal*; present
    only in the plugin is an *addition*; present in both but different is a
    *change*, which replaces the master's entries for that form with the
    plugin's. A re-level or re-count is therefore one change of that form,
    never a removal plus an independent addition, so two plugins moving the
    same sword to different levels cannot leave it in the list twice.
  - *Conflicts resolve last-wins per form.* When independent plugins apply
    different operations to the same form, the later plugin in profile
    order wins for that form, exactly as scalar fields do, and the losing
    operation is reported. Operations on different forms combine. This is
    the Wrye Bash model: its leveled-list patcher keys entries by the
    entry's form, a `Relev`-tagged plugin replaces every entry of each form
    it lists with its own entries for that form, and lists are merged in
    load order so the later `Relev` wins
    (`AMreLeveledList.merge_list` in [common_records.py](https://github.com/wrye-bash/wrye-bash/blob/dev/Mopy/bash/brec/common_records.py);
    "`Relev` tagged plugins can relevel an existing entry (i.e. change its
    levels and/or counts in the list)", [readme](https://wrye-bash.github.io/docs/Wrye%20Bash%20Advanced%20Readme.html#patch-lists)).
    One difference is deliberate: Wrye Bash drops an untagged plugin's
    addition of a form that is already present ("If such a plugin adds an
    entry that already exists, it will be ignored"); here it is a change
    and resolves last-wins, so no plugin's intent is dropped silently.
    Removals need no tag: the diff supplies what `Delev` declares.
  - *Scalar fields.* The record's non-list fields (chance none and its
    global, the calculation flags, model, record flags, every scalar
    property) follow last-wins among the same independent set, with a
    dependent override winning wholesale, exactly as LMC-REC-002.
  - *Explainability.* Every merged record MUST be explainable per form and
    per scalar field: which plugin's operation won, which lost, and under
    which rule.
  The recommended default is `merge`; `legacy` restores LMC-REC-002 for
  every record. This makes Bashed-/Smashed-/Synthesis-style list patches
  redundant for the cases it covers.
- **LMC-REC-004.** DELE-flagged overrides and tombstoned references MUST
  behave as the game treats them: a deleted base record disappears from the
  merged index; a deleted reference is absent from its cell. The cleaning
  convention of "undeleting then disabling the references and moving it to
  a position where it is no longer visible"
  ([Tome of xEdit §7](https://tes5edit.github.io/docs/7-mod-cleaning-and-error-checking.html))
  produces data the engine already handles as a disabled, moved reference.
- **LMC-REC-005.** Injected records (a plugin defining a form whose ID space
  belongs to one of its masters) MUST resolve to the master's identity space
  with the defining plugin recorded as provenance.
- **LMC-REC-006 — Identical overrides.** "Identical to master" is defined
  relative to the record the override would replace at its position in the
  fold (the current winner), not relative to its master. An override that
  equals the current winner is a no-op: it MUST NOT take ownership for
  merge-policy purposes and SHOULD be reported as such. An override that
  equals its master but differs from the current winner reverts an earlier
  plugin's change. That is xEdit's ITM, "a type of dirty edit made where
  the edited value(s) is/are identical to the value(s) without the edit",
  whose consequence for the earlier plugin is that "it will have its effect
  cancelled out by the ITM record"
  ([Tome of xEdit §7](https://tes5edit.github.io/docs/7-mod-cleaning-and-error-checking.html)),
  and some patches do it on purpose. Such an override keeps ownership and
  MUST be reported as a revert naming the plugin whose change it undid.
- **LMC-REC-007.** The conflict report lists, per form: winner, losers in
  order, the policy that chose, and the fields that differ. This is the
  `DataStore` provenance model ([plugin-loading.md](../engine/plugin-loading.md#datastore))
  applied at the live fold, which today produces none of it.
- **LMC-REC-008.** Built-in record errata apply after official content and
  before the first user plugin ([builtin-fixes.md §5.1](../engine/builtin-fixes.md#51-record-errata-after-official-content-before-user-plugins)).
  The fold loop MUST learn the official/user boundary; it does not have one.
- **LMC-REC-009.** A plugin's game dialect comes from its header and the
  profile's game, never from consumer branches
  ([per-game translation rule](../engine/nif-engine-translation-layer.md)).
- **LMC-REC-010.** Sub-records the parser does not decode MUST be preserved
  as opaque bytes with provenance, so a later decoder sees them and the
  explain tooling can show that a mod touched them
  ([ARC-010](../engine/sandboxed-linked-mods.md#71-package-content-and-attachment)).
- **LMC-REC-011.** Cell overrides merge per cell, with the winning plugin's
  placed references replacing the loser's for the same reference ID and new
  references appended; navmesh overrides take the winner's navmesh for that
  cell. Navmesh island reconciliation is not required by this document.
- **LMC-REC-012 — Precombines are never required.** When any plugin in the
  profile overrides a cell's references or its precombine list, the engine
  MUST render that cell from its placed references and MUST NOT depend on
  previs or occlusion data for correctness. Precombined geometry is a
  load-time optimisation the engine may use when intact, never an input a
  mod has to regenerate.

### 5.3 Assets (`LMC-AST`)

- **LMC-AST-001.** Loose files outrank every archive (shipped, #6200).
- **LMC-AST-002 — Paired archives.** For each enabled legacy plugin the
  engine MUST open the archives the game would pair with it by name, in
  profile order. The per-game rules, with their verification state:
  - *Oblivion* (verified): `<PluginName><anything>.bsa`, where `<anything>`
    is empty or starts with a space, hyphen or underscore, and the plugin
    "must be an ESP file"
    ([UESP: BSAs and Archive Invalidation](https://en.uesp.net/wiki/Oblivion_Mod:BSAs_and_Archive_Invalidation)).
    Vanilla archives come from the INI `sArchiveList`.
  - *FO3 / FNV* (exact name verified): `JunkyMod.esp` loads `JunkyMod.bsa`,
    and "the above method doesn't work for .esm files"
    ([GECK: BSA Files](https://geck.uesp.net/wiki/BSA_Files)). **Phase 0:
    verify** the prefix form (`Foo - Main.bsa`, `Foo - Sounds.bsa`) that FNV
    mods commonly ship; it is community-reported only.
  - *Skyrim LE / SE* (**Phase 0: verify**): community sources describe
    `<stem>.bsa` and `<stem> - Textures.bsa` as the two paired names; no
    primary source was found. Vanilla archives come from
    `sResourceArchiveList` / `sResourceArchiveList2`, "a list of BSAs to be
    loaded without a plugin"
    ([STEP: Skyrim INI](https://stepmodifications.org/wiki/Guide:Skyrim_INI/Archive)).
  - *FO4* (verified): `<stem> - Main.ba2` and `<stem> - Textures.ba2`;
    "Archive files require a Data File of the same name to be activated"
    ([FO4 CK: Archive File](https://falloutck.uesp.net/wiki/Archive_File)).
    **Phase 0: verify** the localised voice-archive suffix.
  - *Starfield* (suffixes verified): `- Main`, `- Main_Xbox`, `- Textures`,
    `- Textures_Xbox` ([Archive2](https://starfieldwiki.net/wiki/Starfield_Mod:Archive2)).
    **Phase 0: verify** voice and localisation suffixes, and whether pairing
    is automatic or needs a plugin of the same name.
  Numeric siblings the engine already pairs stay paired. Archive-versus-
  archive precedence: the game's official archive list first, then paired
  archives in profile plugin order. Loose files outrank all of them
  (LMC-AST-001, LMC-AST-010). Not implemented today.
- **LMC-AST-003.** Archive-list semantics the games read from INI and `.ccc`
  are imported into the profile (LMC-ID-006) and reproduced by profile
  order; the files themselves are not read at runtime.
- **LMC-AST-004.** Archive-invalidation mechanisms are D2: the INI flag is
  ignored and a redirection archive is skipped, each with a note. They
  exist because Oblivion and FO3/FNV need help "to recognize certain new
  files placed in the /Data/ folder"
  ([GECK: BSA Files](https://geck.uesp.net/wiki/BSA_Files)), and
  `bInvalidateOlderFiles` "originated with Oblivion"
  ([STEP: Skyrim INI](https://stepmodifications.org/wiki/Guide:Skyrim_INI/Archive)).
- **LMC-AST-005.** Asset keys normalise case, separators and the `Data\`
  prefix, and include every asset root a target game uses (`textures`,
  `meshes`, `materials`, `geometries`, plus `sound`, `music`, `scripts`,
  `interface`, `strings`, `facegendata`, `lodsettings` as those consumers
  move to the shared provider). Today the loose index covers four roots.
- **LMC-AST-006.** Asset errata from the fix catalog apply only when the
  vanilla content hash matches ([FIX-006](../engine/builtin-fixes.md#1-requirements)),
  so a mod's replacement mesh is never "fixed" into something else.
- **LMC-AST-007.** The texture loader MUST accept every DDS layout the target
  game's own assets use and the layouts common mod tooling emits for that
  game. The corpus (§7) gates this; the renderer's own doc lists the current
  set.
- **LMC-AST-008.** Mod-authored NIFs join the NIF corpus gate. A mesh the
  parser cannot recover is a parser bug, not a mod bug, unless the file is
  provably malformed.
- **LMC-AST-009.** Every audio container the games accept (`.wav`, `.xwm`,
  `.mp3` for Oblivion, `.fuz`, `.lip`) loads from loose or archived mod
  content with the same precedence as other assets.
- **LMC-AST-010 — Mod-root order.** A profile has one loose root per mod,
  and the profile MUST carry those roots as an ordered list, independent of
  plugin order. That order resolves loose-versus-loose conflicts: the later
  root wins, which is the rule mod managers encode ("the one loaded at
  higher numeric priority will overwrite the change of lower numeric
  priority", [STEP: Mod Organizer guide](https://stepmodifications.org/wiki/Guide:Mod_Organizer)).
  Across tiers the rule is fixed: any loose file outranks any archive at
  the same path, so one mod's archive never beats another mod's loose file,
  whatever their orders. Archive-versus-archive follows LMC-AST-002. This
  is the games' own default (traditionally "BSAs were always overwritten"
  by loose files, same guide); MO2's managed-archives mode, which ranks
  archives by mod order, is not reproduced (LMC-ID-006). Each root's
  contents are known from LMC-ID-012, so a loose file that shadows another
  root's script or asset is attributed to the winning root (LMC-DX-005).
  LMC-ID-006 populates the root order and ownership on import; LMC-DX-001
  explains every winner.

### 5.4 Scripts (`LMC-SCR`)

- **LMC-SCR-001.** Mod scripts load from loose `Scripts/*.pex` and from any
  enabled archive under the asset precedence rules. The `--scripts-bsa`
  flag becomes a profile-derived list.
- **LMC-SCR-002.** A mod `.pex` that overrides a vanilla script wins by asset
  precedence, and a fingerprint-keyed script erratum skips it
  ([builtin-fixes.md §5.2](../engine/builtin-fixes.md#52-script-errata-skyrim)).
- **LMC-SCR-003.** A call the lowering declines goes to the VM; a call the VM
  does not model produces an attributed diagnostic naming the mod, script,
  function and line, and the script continues where that is safe. Silence
  is forbidden (SDK plan, "graceful compatibility").
- **LMC-SCR-004.** Extender script functions are catalogued per function with
  a disposition (`Native` / `Mapped` / `Unsupported` in
  `crates/sdk/src/compatibility/`). The catalog is published in the docs
  with coverage counts per family (SKSE, F4SE, xNVSE, JIP LN, OBSE, SFSE)
  and per game. It MUST also record, per function, the API level of its
  family that introduced it, so LMC-SCR-010 can answer probes level by
  level; the GECK wiki carries this per command ("Added by NVSE 1.1",
  [GetNVSEVersion](https://geck.uesp.net/wiki/GetNVSEVersion)), and
  populating it for every family from the extenders' and libraries'
  changelogs is Phase 0 work. Wave order follows corpus demand (§9,
  Phase 3).
- **LMC-SCR-005 — Eager preflight.** Preflight scans every script in the
  profile, archived and loose, not only those observed at attach time.
  Today the registry is observed-only
  ([scripting.md](../engine/scripting.md#pex-extender-compatibility-preflight)).
- **LMC-SCR-006.** Papyrus VM tuning INI keys are reported D2.
- **LMC-SCR-007.** Script state lives in components and the VM's bounded
  save-backed state; no stack is serialised. State owned by a removed mod
  follows LMC-SAV-002.
- **LMC-SCR-008.** NVSE/OBSE language extensions (S3) are tracked D5 with a
  per-construct catalog, the same way functions are. They are promoted to D1
  construct by construct, never by a dialect flag.
- **LMC-SCR-009.** The script coverage metric is the fraction of corpus
  scripts that attach with zero unsupported-call diagnostics, per game,
  alongside the existing lowering and VM percentages.
- **LMC-SCR-010 — Version and installation probes (decision Q3).** Mods
  gate on probes and disable themselves when the answer is "absent", and
  many call newer functions only inside a branch the probe itself guards,
  falling back to older ones otherwise; the GECK wiki documents exactly that
  pattern (`if GetNVSEVersion > 4 || GetNVSERevision >= 5 … else … call
  MyLegacyCodeUDF`, [Supporting Multiple Versions of NVSE](https://geck.uesp.net/wiki/Supporting_Multiple_Versions_of_NVSE)).
  A flat "every call covered, or absent" rule would disable such a mod over
  a call it never makes at a lower level, so the rule is level-aware:
  - The catalog records, per function, the API level of its family that
    introduced it (LMC-SCR-004).
  - Because LMC-SCR-005 scans every script eagerly and LMC-ID-012 says
    which scripts a mod owns once loose overrides are resolved, the engine
    knows each mod's complete call set per family before launch.
  - Each mod receives, per family, the highest API level at which every
    function it calls from that level or below is covered by the catalog.
    If even the lowest level has an uncovered call, the answer is "absent",
    so the mod self-disables cleanly instead of half-working. Calls above
    the chosen level stay reported as unsupported, and the preflight states
    the level chosen and the calls that capped it (LMC-DX-001).
  - For ObScript families the reporting unit is the script, not the mod:
    in the original a script "will fail during game if [it references] any
    features added by an NVSE version later than that installed"
    ([GetNVSEVersion](https://geck.uesp.net/wiki/GetNVSEVersion)), so a
    script with a call above the mod's chosen level is reported as a whole,
    which is what its author would have seen.
  Today every probe answers as feature-discovery evidence and never returns
  a version. The probes in scope, verified against their sources:
  - SKSE: `GetVersion`, `GetVersionMinor`, `GetVersionBeta`,
    `GetVersionRelease`, `GetScriptVersionRelease`, `GetPluginVersion(name)`
    ([CK: SKSE Script](https://ck.uesp.net/wiki/SKSE_Script)).
  - F4SE: `GetVersion`, `GetVersionMinor`, `GetVersionBeta`,
    `GetVersionRelease`, `GetScriptVersionRelease`
    ([FO4 CK: GetVersion - F4SE](https://falloutck.uesp.net/wiki/GetVersion_-_F4SE)).
  - xNVSE: `GetNVSEVersion`, `GetNVSERevision`, `GetNVSEBeta`,
    `IsPluginInstalled "<name>"`, `GetPluginVersion "<name>"`, where the
    name is the plugin's registered name such as `"JIP NVSE Plugin"`
    ([GECK: IsPluginInstalled](https://geck.uesp.net/wiki/IsPluginInstalled),
    [GetPluginVersion](https://geck.uesp.net/wiki/GetPluginVersion)).
  - OBSE: `GetOBSEVersion`, `GetOBSERevision`, `IsPluginInstalled`,
    `GetPluginVersion` ([CS: GetOBSEVersion](https://cs.uesp.net/wiki/GetOBSEVersion),
    [IsPluginInstalled](https://cs.uesp.net/wiki/IsPluginInstalled)).
  - Libraries served as D4 services: PapyrusUtil `GetVersion()` (native)
    and `GetScriptVersion()`
    ([PapyrusUtil.psc](https://github.com/eeveelo/PapyrusUtil/blob/master/Scripts/Source/PapyrusUtil.psc));
    powerofthree's Papyrus Extender `GetPapyrusExtenderVersion()`
    ([wiki](https://github.com/powerof3/PapyrusExtenderSSE/wiki));
    JContainers `APIVersion()`
    ([source](https://github.com/SilverIce/JContainers/blob/develop/JContainers/src/api_3/tes_jcontainers.h)).
  - **Phase 0: verify** the probe functions of MCM Helper, Fallout 4 MCM,
    ConsoleUtil, UIExtensions, HUDFramework, Workshop Framework and SFSE;
    none was confirmed from a primary source for this draft.
  The version a mod receives is the API level the catalog covers for that
  family, never a fabricated build number. Q3 records the alternatives.

### 5.5 Native extensions (`LMC-EXT`)

- **LMC-EXT-001.** No native binary is loaded (LMC-NG-001). A profile that
  contains `SKSE/Plugins/*.dll`, `F4SE/Plugins/*.dll`, `NVSE/Plugins/*.dll`
  or `OBSE/Plugins/*.dll` MUST report each file with its disposition and,
  where known, its replacement (catalog entries, service, or port guide).
- **LMC-EXT-002.** Engine-fix plugins map to fix-catalog entries. The report
  for one says which entries replace it and which of its fixes are still
  `pending` in the catalog, so "redundant" is a checked claim.
- **LMC-EXT-003.** A script-library DLL whose Papyrus API is depended on by
  corpus mods is served by an engine-owned compatibility service with an
  explicit unsupported subset (the StorageUtil and JContainers pattern).
  Threshold and order are set by corpus demand in Phase 0.
- **LMC-EXT-004.** Gameplay DLLs are ported to the SDK. The porting guide
  ([CPP-008](../engine/sandboxed-linked-mods.md#16-c-and-c-sdk-requirements)) maps
  hooks to events and services and attached memory to extension schemas,
  and ships with at least one worked port of a source-available plugin.
- **LMC-EXT-005.** A ported extension reads its mod-provided configuration
  files (T8) through scoped content read; the engine never interprets
  those files on a mod's behalf.
- **LMC-EXT-006.** Compatibility services implement published *interfaces*
  (the `.psc` declarations and documented lifetimes), anchored by citation,
  never copied implementations (LMC-LEGAL-003).

### 5.6 UI (`LMC-UI`)

- **LMC-UI-001.** A replacement `.swf` from a mod loads through asset
  precedence and runs inside the host bridge; every host method it calls
  that the catalog lacks is an attributed diagnostic. Per-menu coverage is
  published from the catalogs (`ui.md`).
- **LMC-UI-002 — MCM host.** The engine owns a Mod Configuration Menu host
  that serves the SkyUI MCM Papyrus contract (`SKI_ConfigBase`), MCM Helper's
  declarative JSON, FO4 MCM's JSON and FNV MCM's NVSE-era registration, and
  persists MCM settings through the extension-state save path
  ([STATE-001](../engine/sandboxed-linked-mods.md#13-persistent-state-and-saves)).
  Today only `UI.IsMenuOpen` is served.
- **LMC-UI-003.** MenuXml replacements, `<include>` prefabs and font
  replacements from mods load through the MenuXml crate with the same
  precedence rules.
- **LMC-UI-004.** Translation files are honoured for Scaleform text
  replacement.
- **LMC-UI-005.** HUD frameworks that add widgets by Scaleform injection are
  D4; the semantic UI contribution service
  ([replacement map](../engine/sdk-v0.1-development-plan.md#7-script-extender-replacement-map))
  is their target.

### 5.7 Settings (`LMC-SET`)

- **LMC-SET-001.** Engine behaviour is configured by the settings registry
  (`crates/core/src/settings`), not by game INIs. A documented table maps
  the INI keys the engine honours on import (gameplay-visible values such
  as field of view, difficulty, streaming radius) to registry entries; every
  other key is D2 (removed limit) or D3 (graphics) with a note.
- **LMC-SET-002.** GMST overrides from plugins are the authoritative source
  for every rule the games expose as a GMST; consumers read the merged
  index (the `VatsModel` / `PoolRegenConfig` pattern, re-derived from the
  load order), never constants.
- **LMC-SET-003.** Mod-provided INIs and JSON are mod data (LMC-EXT-005).
- **LMC-SET-004.** Settings a mod writes at runtime (MCM, extension settings
  declarations) persist per profile, not per installation.

### 5.8 Built-in fixes (`LMC-FIX`)

[Built-in Fixes](../engine/builtin-fixes.md) owns the requirements. This document
adds the user-facing contract:

- **LMC-FIX-001 — Known patches are not loaded (decision Q1).** When a
  profile contains an unofficial patch the fix catalog names for that game
  (§3.1 row T1), activation MUST NOT load it, and the report MUST tell the
  user it is not needed. The report MUST also count the patch's edits that
  the catalog covers and list those it does not; an edit on that list is not
  applied, so each one is a catalog gap, counted in the compatibility
  metric until the catalog covers it.
- **LMC-FIX-002.** A fix is never applied to content a mod changed
  (FIX-006), and the explain tooling shows which mod disarmed which fix.
- **LMC-FIX-003.** `intent`-class fixes follow decision Q7.
- **LMC-FIX-004 — Stand-in for a skipped patch (decision Q1).** A patch
  skipped under LMC-FIX-001 MUST appear installed to everything that asks:
  it satisfies master lists (so LMC-ID-011 does not disable its dependents),
  and the load-order queries that mods use as presence probes
  (`IsModLoaded`, `GetModIndex`, `GetNthModName` and their families,
  LMC-SCR-010) answer as if it were loaded. A record the patch adds and an
  enabled mod references MUST resolve to an engine-provided record under
  the patch's identity. A reference the engine cannot yet resolve is
  reported per mod, with the record it names, and counted as a catalog gap;
  the mod is not disabled for it. How the engine provides those records
  without redistributing the patch (FIX-002) is a Phase 0 design item.

### 5.9 Animation and body (`LMC-ANIM`)

- **LMC-ANIM-001.** Behaviour graphs and animation files produced by FNIS,
  Nemesis or Pandora load as assets and drive the actor the same way vanilla
  graphs do. This is bound to the behaviour-graph runtime, which the
  feature matrix still lists as a gap for ambient locomotion.
- **LMC-ANIM-002 — Conditional replacement.** The engine reads the DAR
  `_CustomConditions` folder convention and the OAR `config.json` convention
  natively and selects clips per actor by their condition sets. No DLL. This
  is the engine-native equivalent of the most depended-on animation
  mechanism on Skyrim.
- **LMC-ANIM-003.** Skeleton replacements (XPMSSE-style NIF skeletons) are
  assets (D1). The character-editor plugins that ship beside them stay D5
  until runtime morphs exist (N9).
- **LMC-ANIM-004.** Body and outfit meshes from BodySlide-style tooling are
  assets (D1); `.tri` morph sets are D5 until runtime morphs exist.
- **LMC-ANIM-005.** Per-NPC head and face asset overrides (the Blockhead
  mechanism) are engine-native: a profile-level or plugin-level override
  table keyed by stable form identity.

### 5.10 Saves and profile changes (`LMC-SAV`)

- **LMC-SAV-001.** A save stores the profile fingerprint (LMC-ID-007) and the
  ordered plugin list with content hashes.
- **LMC-SAV-002 — Removing a mod.** Loading a save whose profile contained a
  plugin now absent MUST load. References whose base form belonged to the
  absent plugin are despawned; inventory items whose base form is absent are
  removed and listed; script and extension state owned by the absent
  package is preserved as opaque within quota and purgeable
  ([STATE-008](../engine/sandboxed-linked-mods.md#13-persistent-state-and-saves)).
  The load report lists every consequence. The engine MUST NOT refuse a load
  solely for an absent plugin, and MUST NOT silently carry dangling forms.
- **LMC-SAV-003 — Adding a mod.** New records resolve on load; overrides the
  new mod makes apply to the reloaded cell because load reloads cells from
  current data and overlays saved deltas (the M45.1 change-form overlay).
  Two consequences are fixed here. (a) When the new mod changes a reference
  the save already holds a delta for, the saved delta wins, the role the
  original engines' per-form change records play in their own saves
  ([UESP: Skyrim save file format](https://en.uesp.net/wiki/Skyrim_Mod:Save_File_Format));
  the load report names the reference and the mod whose edit the delta
  masks. (b) Start Game Enabled quests the new mod adds start on the first
  load of that save, from the QUST record and with no SEQ file (R11); the
  original engines needed one to notice such quests on an existing save
  ([CK: Quest Data Tab](https://ck.uesp.net/wiki/Quest_Data_Tab)). Mods that
  change already-visited cells therefore take effect on existing saves,
  which the original engines could not guarantee.
- **LMC-SAV-004 — Reordering mods.** No Form ID shifts by construction
  (stable `FormIdPair`). A reorder changes which override wins, and the load
  report says so for records the save references.
- **LMC-SAV-005.** Mod-version migration of extension state follows
  [STATE-006](../engine/sandboxed-linked-mods.md#13-persistent-state-and-saves);
  Papyrus-only mods keep component state keyed by script identity.
- **LMC-SAV-006.** No legacy `.ess` / `.fos` import and no cosaves
  (LMC-NG-003).

### 5.11 Tooling and explainability (`LMC-DX`)

- **LMC-DX-001 — Preflight.** A headless command (launcher screen and CLI)
  checks a profile without a renderer: per mod, per mechanism, disposition,
  with the root that owns each file (LMC-ID-012); missing masters, the
  plugins disabled for them and the roots disabled or kept as a result
  (LMC-ID-011); identity collisions; record conflicts and chosen winners,
  including reverts (LMC-REC-006) and losing list operations (LMC-REC-003);
  for every asset path claimed by more than one root or archive, the
  winning root or archive and each loser (LMC-AST-010); unsupported script
  calls and, per mod and family, the probe answer chosen (the API level or
  "absent") with the calls that capped it (LMC-SCR-010); DLLs and INIs with
  their dispositions; redundant items; per-game coverage summary. It MUST
  run without activating executable code
  ([DX-010](../engine/sandboxed-linked-mods.md#17-diagnostics-and-developer-experience)).
- **LMC-DX-002.** At runtime the debug CLI exposes the `mod.*` family
  ([DX-008](../engine/sandboxed-linked-mods.md#17-diagnostics-and-developer-experience)),
  `sdk.compat` (exists) and the fix report
  ([FIX-009](../engine/builtin-fixes.md#1-requirements)).
- **LMC-DX-003.** A conformance runner drives the corpus headlessly per game,
  opt-in like the game-data corpus tests, with the mod files supplied
  locally and never checked in.
- **LMC-DX-004.** The published per-game compatibility matrix grows from the
  current `COMPATIBILITY.md` §15 rows into per-mechanism rows with the
  corpus pass rate. One home (ROADMAP), refreshed at session close.
- **LMC-DX-005.** Every diagnostic attributes to a package identity. "Unknown
  mod" is not an acceptable attribution. When a script or asset was
  shadowed by another root's loose file, the diagnostic attributes to the
  winning root, whose copy is the one that ran or rendered, and names the
  original author's root as shadowed (LMC-ID-012, LMC-AST-010).
- **LMC-DX-006.** The preflight output is stable and machine-readable so
  mod authors can run it in their own CI against the engine.

### 5.12 Performance (`LMC-PERF`)

- **LMC-PERF-001.** A profile of 1,000 legacy plugins with paired archives
  activates; activation time is published with hardware and cache state
  ([SCALE-011](../engine/sandboxed-linked-mods.md#14-performance-and-scale)). Per-plugin
  parsing already runs in parallel (#3813).
- **LMC-PERF-002.** Cost of the merged index is linear in records. A cell
  transition MUST NOT re-parse the load order on the main thread (#6591 is
  open against this).
- **LMC-PERF-003.** Asset lookup is an index hit, independent of archive
  count.
- **LMC-PERF-004.** Preflight on a 1,000-plugin profile completes in a time
  a launcher can show a progress bar for, published like LMC-PERF-001.

### 5.13 Legal (`LMC-LEGAL`)

- **LMC-LEGAL-001.** The engine ships no mod, patch or fix content. The
  corpus is local and opt-in.
- **LMC-LEGAL-002.** Every re-derived fix is cited
  ([FIX-003](../engine/builtin-fixes.md#1-requirements)).
- **LMC-LEGAL-003.** Compatibility services are written to published
  interfaces (script declarations, documented lifetimes, format
  specifications) and cite them, as the StorageUtil and JContainers services
  do; they do not copy implementations.

---

## 6. Wrongs righted, and what each one means for compatibility

Each row removes a legacy limit, names the ecosystem workaround that
becomes redundant, and names the compatibility risk the removal creates.
The risk column is why this document exists: removing a limit silently is
how a clean rebuild breaks mods that assumed it.

| # | Legacy limit | Redux behaviour | Becomes redundant | Compatibility risk and its requirement |
|---|---|---|---|---|
| 1 | 254 regular + 4,096 small plugin slots, plus 256 medium slots on Starfield ([Ortham, Load order in Starfield](https://blog.ortham.net/posts/2024-06-28-load-order-in-starfield/)) | No ceiling (LMC-ID-004) | Merged plugins, ESL-flagging, "plugin limit" patches | Scripts and tools that assume 8-bit indices → virtual index (LMC-ID-005) |
| 2 | Form IDs bound to load-order position | Stable `FormIdPair` | Form ID renumbering, save-game re-indexing tools | Scripts with literal runtime Form IDs → LMC-ID-005 |
| 3 | Rule of one (whole-record override) | Baseline kept (LMC-REC-002); multiset list merge policy (LMC-REC-003) | Bashed / Smashed / Synthesis list patches, `Delev` / `Relev` tagging | A mod that *intends* to strip a list → removals honoured, dependents win wholesale; documented "load after" advice is exact only under `legacy`; decision Q2 |
| 4 | Dirty edits, ITMs, UDRs | Identical-to-winner overrides are no-ops; an override that reverts an earlier plugin's change keeps ownership and is reported as a revert (LMC-REC-006); tombstones handled (LMC-REC-004) | Plugin cleaning | A deliberate revert is never cleaned away; it is reported so the user sees which plugin it undid |
| 5 | Papyrus VM queue lag, stack dumps, memory caps | ECS lowering + bounded VM, no stack serialisation | Papyrus INI tuning, script-cleaner tools | Scripts relying on VM ordering races → ordering documented in scripting.md; diagnostics (LMC-SCR-003) |
| 6 | Save bloat, orphaned scripts, "never uninstall mid-save" | Snapshot saves, orphan policy (LMC-SAV-002) | ReSaver / FallrimTools | None (no legacy saves) |
| 7 | Physics and timers bound to frame rate | Fixed-step physics | High FPS Physics Fix, display-tweak physics halves | A behaviour fix in the catalog, tested (FIX-008) |
| 8 | `uGridsToLoad` and cell-buffer hacks | Streaming radius is a setting | uGrids guides, "stable uGrids" plugins | None |
| 9 | Precombine / previs fragility (FO4) | REFR-first on override (LMC-REC-012) | PRP, "disable previs" | None; performance of REFR-first cells measured |
| 10 | 32-bit address space | 64-bit Rust | 4GB patch, heap replacers, allocators | None |
| 11 | Archive invalidation | Loose wins (#6200) | Invalidation tools and dummy archives | None |
| 12 | Windows-only case-insensitive paths | Case-folded keys (LMC-ID-009) | Case-fixing scripts under Proton | None |
| 13 | 255 masters per plugin | No ceiling | Master-trimming | None |
| 14 | UI extensibility only by Scaleform injection | Semantic UI service, MCM host (LMC-UI-002/005) | SKSE/F4SE UI halves of SkyUI, MCM, HUD frameworks | Those mods' DLL halves are D4 until the service lands |
| 15 | Per-object state only via ExtraData hacks | Extension components, principal storage | PapyrusUtil / JContainers DLLs | File-backed JSON state unsupported today → decision Q5 |
| 16 | Hard-coded engine formulas ignoring GMST edits | GMST-driven rules (LMC-SET-002) | Tweak DLLs | Engine hard-codes are bugs (#6567 class) |
| 17 | Head/face overrides need a DLL (Oblivion) | Native override table (LMC-ANIM-005) | Blockhead | OCOv2-class mods depend on it until built |
| 18 | Conditional animations need a DLL | Native condition-set clip selection (LMC-ANIM-002) | DAR / OAR | Animation mods depend on it until built |
| 19 | Mods disable themselves when the extender is absent | Version-probe policy (LMC-SCR-010) | — | Decision Q3 |
| 20 | Mod managers needed for a virtual `Data` tree | Profile import (LMC-ID-006) | MO2 / Vortex at runtime | Users keep their manager for downloads; the engine reads its output |
| 21 | Mods disabled by executable version checks | No version-specific binaries | Address Library, version-locked DLL builds | None |
| 22 | Editing needs the Creation Kit / GECK | M50 editor (out of this document's scope) | — | — |
| 23 | A missing master crashes the game at startup ("CTD on Startup usually indicates a missing master", [UESP: Oblivion troubleshooting](https://en.uesp.net/wiki/Oblivion_Mod:Troubleshooting); later games community-reported, Phase 0: verify) | Disable the plugin and its dependents, report, continue (LMC-ID-011) | Mod-manager "missing master" gates used as crash guards | A disabled dependent is reported, never silently skipped |
| 24 | Start Game Enabled quests added by a mod need a generated SEQ file ([CK: Quest Data Tab](https://ck.uesp.net/wiki/Quest_Data_Tab)) | Started from the QUST record on first load (LMC-SAV-003) | "Generate SEQ" tooling steps | None; a shipped `seq/` file is ignored with a note (R11) |

---

## 7. Per-game scope and the conformance corpus

### 7.1 Waves

Mod-compatibility waves follow the engine tiers in
[game-compatibility.md](../engine/game-compatibility.md#per-game-support-detail) and
pick one title per lineage first, so each wave exercises a different
script language, UI stack and archive family:

| Wave | Games | Why |
|---|---|---|
| 1 | Fallout: New Vegas, Skyrim SE | Engine reference title (ObScript, MenuXml, BSA v104, xNVSE/JIP ecosystem) and the largest modern ecosystem (Papyrus, Scaleform, BSA v105, SKSE ecosystem) |
| 2 | Fallout 4, Fallout 3 | BA2, precombines, F4SE; FO3 shares FNV's machinery |
| 3 | Oblivion, Starfield | Tier 3 and Tier 2 engine support respectively |

Skyrim LE is counted with SE: the same mods ship for both and the engine
reads both releases' archives. Mods that exist only for LE are not a
separate wave.

### 7.2 Candidate corpus

A starting inventory, chosen so that every mechanism in §3.1 is exercised
at least once per game and the most depended-on masters are present. Each
entry MUST be verified in Phase 0 (it exists, its licence permits local
testing, it exercises the mechanisms listed) before it counts. The list is
not a compatibility claim about any mod.

| Game | Candidate | Mechanisms exercised |
|---|---|---|
| FNV | Yukichigai Unofficial Patch | T1, R2, S2 |
| FNV | JIP LN NVSE, xNVSE | N1, S3, S4 (catalog demand) |
| FNV | The Mod Configuration Menu + UIO | U2, U3 |
| FNV | Vanilla UI Plus | U3 |
| FNV | Project Nevada | S2, S3, S4, U3 |
| FNV | Weapon Mods Expanded | R1, R2, A2 |
| FNV | A World of Pain / NVInteriors | R5, A2 |
| FNV | New Vegas Bounties | S2, R5, A7 |
| FNV | Nevada Skies | R1, R2 (weather) |
| FNV | NMC's Texture Pack, EVE | A1, A2, A5, A6 |
| FNV | NVAC, NVTF, lStewieAl's Tweaks | N2, N3 (D2/D3 reporting) |
| FNV | Tale of Two Wastelands | T12 (decision Q6) |
| Skyrim SE | Unofficial Skyrim Special Edition Patch | T1, R2, S1 |
| Skyrim SE | SKSE64, Address Library, SSE Engine Fixes, Bug Fixes SSE | N1, N2, N3 (D2/D3 reporting) |
| Skyrim SE | SkyUI, MCM Helper | U1, U2, U4, S4, S5 |
| Skyrim SE | PapyrusUtil, JContainers, powerofthree's Papyrus Extender, ConsoleUtil | S5 |
| Skyrim SE | Frostfall + Campfire | S1, S4, S5, U2 |
| Skyrim SE | Ordinator | R2, S1, S7 (perk fragments) |
| Skyrim SE | Cutting Room Floor, Alternate Start | S1, S7, R5 |
| Skyrim SE | Legacy of the Dragonborn | S1, S7, R5, A2 (scale) |
| Skyrim SE | Beyond Skyrim: Bruma | R5 (new worldspace), A2, A7 |
| Skyrim SE | ELFX, Obsidian Weathers | R2 (lighting, weather) |
| Skyrim SE | SMIM, Noble Skyrim | A1, A2, A5, A6 |
| Skyrim SE | XPMSSE, Nemesis or Pandora output, OAR + an OAR-based animation mod | N6, N7, LMC-ANIM-* |
| Skyrim SE | A mod shipping `seq/<plugin>.seq` for a Start Game Enabled quest with dialogue, chosen in Phase 0 | R11, LMC-SAV-003 |
| Skyrim SE | TexGen + DynDOLOD output built over the set (DynDOLOD DLL NG present, never loaded) and xLODGen terrain LOD | A10, N1, N6 |
| Skyrim SE | No Grass In Objects NG with a generated grass cache, Grass Cache Helper NG | A11, N2 (D3 reporting) |
| Skyrim SE | A Bashed Patch and a Synthesis patch built over the set | T2 |
| Skyrim SE | Enderal | T12 (decision Q6) |
| FO4 | Unofficial Fallout 4 Patch | T1 |
| FO4 | F4SE, Buffout 4, High FPS Physics Fix | N1, N2, N3 |
| FO4 | MCM, HUDFramework, DEF_UI | U1, U2, U5 |
| FO4 | Workshop Framework, Sim Settlements 2 | S1, S4, S5 (the SDK plan's existing real-mod gate) |
| FO4 | AWKCR | R4 (injected records), R1 |
| FO4 | Previsibines Repair Pack, Scrap Everything | T4, LMC-REC-012 |
| FO4 | Place Everywhere | N4 (port candidate) |
| FO4 | Vivid Fallout | A1, A2, A5 |
| FO4 | xLODGen (FO4LODGen) terrain LOD output built over the set | A10 |
| FO3 | Unofficial Fallout 3 Patch, FOSE | T1, N1 |
| FO3 | DarNified UI F3 | U3 |
| FO3 | FWE, MMM | S2, R2, R1 |
| Oblivion | Unofficial Oblivion Patch, OBSE, Engine Bug Fixes, MenuQue, Blockhead | T1, N1, N2, N4, N8 |
| Oblivion | DarNified UI | U3 |
| Oblivion | Oscuro's Oblivion Overhaul, Midas Magic | R1, R2, S2 |
| Oblivion | Oblivion Character Overhaul v2 | N8, A9 |
| Oblivion | Better Cities, Unique Landscapes, Qarl's Texture Pack | R5, A1, A2 |
| Starfield | Starfield Community Patch, SFSE | T1, N1 |
| Starfield | StarUI | U1 |
| Starfield | Plugins.txt Enabler, Achievement Enabler | T11 |

### 7.3 Acceptance per corpus mod

A corpus mod passes when:

1. The profile containing it activates with every mechanism reported at the
   disposition the catalog predicts (LMC-CLS-001).
2. Preflight reports no unsupported script call, host method or record
   construct that the catalog does not already list as D5 for that
   mechanism.
3. Its acceptance scenario passes: a scripted check, defined in Phase 0 per
   mod, driven through `--bench-hold` and `byro-dbg` like the existing smoke
   tests. Examples of the shape: *SkyUI* — open the inventory, the MCM lists
   every registered mod page; *YUP* — a named record resolves to the patched
   value and the built-in fix report marks it redundant; *AWOP* — a named
   new interior loads with its references; *Nevada Skies* — a named weather
   cycles in; *Frostfall* — an exposure value changes after a timed wait;
   *PRP + Scrap Everything* — a scrapped cell renders its remaining
   references.
4. Save, reload, and the removal case (LMC-SAV-002) pass for the mod's state.

A mechanism is D1 for a game only when every corpus mod exercising it
passes. A mod that fails a scenario because of a catalogued D5 gap is
reported against that gap, not as a corpus failure.

---

## 8. Metrics

One home for the numbers (ROADMAP's compatibility matrix); this document
defines what is counted.

| Metric | Definition | Granularity |
|---|---|---|
| Mechanism coverage | D1 mechanisms ÷ mechanisms applicable to the game | per game |
| Record coverage | record types consumed by any system that are decoded from non-master plugins ÷ consumed types | per game |
| Script attach rate | corpus scripts attaching with zero unsupported diagnostics ÷ corpus scripts | per game; beside the existing lowering and VM rates |
| Extender API coverage | `Native` + `Mapped` functions ÷ catalogued functions | per family, per game |
| UI method coverage | host methods served ÷ methods called by corpus `.swf` files | per game |
| Corpus pass rate | corpus mods passing §7.3 ÷ corpus mods | per game, per wave |
| Fix parity | `implemented` + `not-applicable` ÷ catalog entries ([builtin-fixes.md §3](../engine/builtin-fixes.md#3-fix-catalog)) | per game |

---

## 9. Delivery sequence

Phases are ordered by what unblocks the most corpus mods; most phases
depend on milestones that already exist. Each phase's exit is a corpus
pass rate for the mechanisms it covers, not a feature list.

**Phase 0 — Catalog and corpus (no engine code).**
Turn §3.1 into the machine-readable catalog (LMC-CLS-006) with one
disposition per mechanism per game, cited. Verify the §7.2 inventory and
write each mod's acceptance scenario. Define the honoured-INI table
(LMC-SET-001). Resolve the §10 decisions. Close every **Phase 0: verify**
item this document carries: the archive-pairing rules per game
(LMC-AST-002), the missing-master behaviour of Skyrim SE, FO4 and Starfield
(LMC-ID-011), the library probe functions and the per-function
"introduced in" API levels of every extender and library family
(LMC-SCR-004, LMC-SCR-010), levelled-list duplicate weighting on Skyrim+
(LMC-REC-003), the SEQ-free quest start on every load path (R11), and
every "verify" cell in §3.1. Design how the stand-in of LMC-FIX-004
provides the records a skipped patch adds without redistributing the
patch (FIX-002), and add the per-game list of known patch plugins to the
fix catalog. Exit: the catalog,
the corpus list and the scenarios are checked in; no mod files are.

**Phase 1 — Mod profile and the record layer (M46).**
Profile model (the sandbox design's `ModProfile`, extended to legacy
plugins, archives and loose roots); discovery; masters-first ordering with
profile-order tie-break (LMC-ID-003); the official/user boundary in the fold
(LMC-REC-008); paired archives (LMC-AST-002); loose roots beyond the current
four (LMC-AST-005); conflict report (LMC-REC-007); list-merge policy
(LMC-REC-003, per Q2); virtual legacy index (LMC-ID-005); profile
fingerprint in saves (LMC-ID-007). Exit: every content-only corpus mod in
Wave 1 activates and passes.

**Phase 2 — Preflight and explainability.**
The headless preflight (LMC-DX-001) with eager script scanning (LMC-SCR-005),
DLL and INI dispositions (LMC-EXT-001, LMC-SET-001), import of legacy load
orders (LMC-ID-006), and the launcher screen for it. Exit: preflight output
for the Wave 1 corpus matches the catalog with no unclassified item.

**Phase 3 — Script compatibility waves.**
The SDK plan's Waves A–D, reprioritised by what the Wave 1 corpus calls
most: the catalog growth per family, the version-probe policy (Q3), the
file-backed storage decision (Q5), the MCM host (LMC-UI-002). Exit: script
attach rate and extender API coverage targets agreed in Phase 0 for Wave 1.

**Phase 4 — Built-in fixes, Phases 0–1 (parallel track).**
[builtin-fixes.md §7](../engine/builtin-fixes.md#7-delivery-sequence) for FNV and
Skyrim SE, plus skipping known patches with their report (LMC-FIX-001,
LMC-EXT-002) and the stand-in for their dependents (LMC-FIX-004). Exit:
fix parity numbers published for both games.

**Phase 5 — Animation and body.**
Modded behaviour graphs through the hkx runtime (LMC-ANIM-001), native
conditional replacement (LMC-ANIM-002), the head-override table
(LMC-ANIM-005). Depends on the M41.x locomotion work. Exit: the animation
rows of the Skyrim corpus pass.

**Phase 6 — Native port programme.**
Porting guide with one worked port of a source-available extender plugin;
compatibility services for the libraries the corpus demands most
(LMC-EXT-003). Exit: at least one corpus D4 mod runs as a port, and the
guide is validated by someone other than its author.

**Phase 7 — Profile-change semantics and the conformance runner.**
LMC-SAV-002/003/004 with their load report; the headless corpus runner in
the opt-in test lane. Exit: Wave 1 corpus pass rate published in ROADMAP;
Wave 2 begins at Phase 1 for its games.

---

## 10. Decisions required

Each needs the project owner. The recommendation is the first option.

1. **Q1 — Reading of "unofficial patches and script extenders work out of the
   box".** This draft reads it as: a user-installed unofficial patch loads
   and wins by load order, nothing requires it; extender-dependent mods run
   without the extender through engine-owned services; the extender binary
   is never loaded. If the intent was instead that both are *out of scope*
   because their job is built in, §3.1 rows T1 and N1 change from D1+D2 to
   D2 only, and LMC-G-003 narrows to the fix facade.
   **Decided** (2026-11-03): neither reading as written. A known unofficial
   patch is never required and is not loaded: the engine carries its fixes
   and tells the user the patch is not needed (LMC-FIX-001). Towards the
   mods that depend on it, the engine mocks its installed state and
   provides its interfaces: the patch satisfies master lists and presence
   probes, and the records it adds resolve to engine-provided records
   (LMC-FIX-004). Script extenders take this draft's reading:
   extender-dependent mods run through engine-owned services and the
   extender binary is never loaded (row N1 and LMC-G-003 unchanged). Row T1
   becomes D2 with the stand-in.
2. **Q2 — List-merge default.** Recommended: `merge` by default with
   `legacy` per profile, using the per-form multiset rules in LMC-REC-003.
   Alternative: `legacy` by default, `merge` opt-in, which keeps documented
   load-order advice exact at the cost of making list patches necessary
   again.
   **Decided** (2026-11-03): the recommendation — `merge` by default,
   `legacy` per profile, per-form multiset rules of LMC-REC-003.
3. **Q3 — Version and installation probes.** Recommended: answer per mod
   and per family, from the eager preflight: the highest API level at which
   every function the mod calls from that level or below is covered, and
   "absent" when even the lowest level has an uncovered call, for extender
   and library probes alike (LMC-SCR-010). Alternative A: one global answer
   per family, the highest API level the catalog covers, with diagnostics
   for calls above it. Alternative B: keep today's policy (no version
   returned), accepting that version-gated mods disable themselves. The
   recommendation and Alternative A both reverse a stated SDK-plan rule and
   need an explicit decision.
   **Decided** (2026-11-03): the recommendation — a per-mod, per-family API
   level from the eager preflight (LMC-SCR-010). This explicitly reverses
   the SDK plan's rule that probes never return a version; the plan cites
   this decision.
4. **Q4 — First wave.** Recommended: FNV and Skyrim SE (§7.1). Alternative:
   Skyrim SE alone, for the largest ecosystem first.
   **Decided** (2026-11-03): the recommendation — FNV and Skyrim SE.
5. **Q5 — File-backed mod state.** JsonUtil, JContainers JSON files and
   similar are "deliberately unsupported" in the SDK plan. Recommended:
   support them confined to a per-mod data directory
   ([SEC-003](../engine/sandboxed-linked-mods.md#102-requirements)) with bounded size,
   because a large share of Skyrim frameworks persist configuration this
   way. Alternative: keep unsupported and port those frameworks' persistence
   to principal storage case by case.
   **Decided** (2026-11-03): the recommendation — supported, confined to a
   per-mod data directory with bounded size. This reverses the SDK plan's
   "deliberately unsupported" for file-backed state; the plan cites this
   decision.
6. **Q6 — Total conversions.** Recommended: Tale of Two Wastelands and
   Enderal are a separate corpus tier measured after Wave 1 passes, not
   gating it. Alternative: exclude them from the corpus.
   **Decided** (2026-11-03): the recommendation — Tale of Two Wastelands
   and Enderal form a separate corpus tier measured after Wave 1 passes.
7. **Q7 — `intent`-class fixes.** Carried from
   [builtin-fixes.md Q2](../engine/builtin-fixes.md#8-open-questions). Recommended:
   on by default, per-profile switch, each entry tagged so a disputed one can
   be turned off alone.
   **Decided** (2026-11-03): the recommendation — on by default,
   per-profile switch, each entry tagged so a disputed one can be turned
   off alone. This closes [builtin-fixes.md
   Q2](../engine/builtin-fixes.md#8-open-questions).
8. **Q8 — Profile import scope.** Recommended: `plugins.txt`,
   `loadorder.txt`, MO2 profiles and Vortex deployments (LMC-ID-006).
   Alternative: `plugins.txt` only.
   **Decided** (2026-11-03): the recommendation — `plugins.txt`,
   `loadorder.txt`, MO2 profiles and Vortex deployments.
9. **Q9 — Blocking versus reporting.** Recommended: data is never blocked;
   code is never loaded; both are reported. Alternative: refuse activation
   when a profile contains a D3 item, which punishes users for a harmless
   leftover.
   **Decided** (2026-11-03): the recommendation — data is never blocked,
   code is never loaded, both are reported.

---

## 11. Document map

| Need | Go to |
|---|---|
| Which fixes are built in, how they apply, how they are cited | [builtin-fixes.md](../engine/builtin-fixes.md) |
| Sandbox, capabilities, WIT, C/C++ SDK, persistence of extension state | [sandboxed-linked-mods.md](../engine/sandboxed-linked-mods.md) |
| Which extender functions are served today, by which route | [sdk-v0.1-next-action-plan.md § Compatibility status table](../engine/sdk-v0.1-next-action-plan.md#compatibility-status-table) |
| Replacement map from extender mechanisms to engine services | [sdk-v0.1-development-plan.md §7](../engine/sdk-v0.1-development-plan.md#7-script-extender-replacement-map) |
| Form ID layers, `DataStore`, resolver, ESM parser | [plugin-loading.md](../engine/plugin-loading.md) |
| Archive precedence and the loose tier | [archives.md](../engine/archives.md#asset-provider-precedence-and-the-loose-tier) |
| Script lowering, VM, preflight | [scripting.md](../engine/scripting.md) |
| Scaleform host bridge and MenuXml | [ui.md](../engine/ui.md) |
| Save format, change-form overlay, validation gates | [save-load-roundtrip.md](../engine/save-load-roundtrip.md) |
| Launcher, boot request, install detection | [launcher.md](../engine/launcher.md) |
| Per-game engine tiers and parse rates | [game-compatibility.md](../engine/game-compatibility.md) |
| Live milestone state and the compatibility matrix | [ROADMAP.md](../../ROADMAP.md) |

---

## 12. Revision and review log

The status line summarises; this table is the record. Review cells use
the verdict vocabulary of the [PRD standard](README.md#8-manual-review):
`revise`, `approved`, `withdrawn`.

| Rev | Date | Change | Review |
|---|---|---|---|
| 1 | 2026-11-01 | First draft: disposition model, mechanism catalog, requirements by layer, wrongs-righted register, candidate corpus, phases, nine decisions | — |
| 2 | 2026-11-01 | Review round 1 folded in | Round 1: revise |
| 3 | 2026-11-01 | Review round 2 folded in; the topics of both rounds are listed in the status line | Round 2: revise |
| 4 | 2026-11-03 | Owner decisions Q1–Q9 recorded; Q1 amended: known unofficial patches are not loaded and the engine stands in for them (thesis 3, T1, LMC-G-002, LMC-FIX-001, new LMC-FIX-004, LMC-ID-011, Phase 0 design item); Q2–Q9 take the recommendation | owner: approved |
| 5 | 2026-11-03 | → ACTIVE: parent #6664 and phase issues #6669, #6670, #6671, #6672, #6673, #6674, #6675, #6676 filed; the Phase 0 verify items are acceptance criteria of #6669 | — |
