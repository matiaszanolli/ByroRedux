# Legacy Mod Compatibility — Product Requirements

**Status**: ACTIVE (2026-11-24). Revision 50 serves SKSE's `Form` reads (type, name, weight, keywords, world model)
from the records and SKSE's form classes. Revision 49 serves SKSE's and F4SE's pure natives (`StringUtil`, `Math` bit operations,
`Utility` arrays; LMC-SCR-004 catalog growth). Revision 48 completes Q5: PapyrusUtil `StorageUtil`'s form-keyed values and `JsonUtil`'s path functions, and JContainers ([design](../engine/jcontainers.md)), in the Papyrus VM. Revision 47 answers the extenders' version probes per mod (LMC-SCR-010, decision Q3):
the level from what a mod's scripts call, in the preflight for every family and at run time for `SKSE` / `F4SE`. Revision 46 serves PapyrusUtil's `JsonUtil` and `StorageUtil.File*` (Q5, [design](../engine/json-util.md)) and SKSE's `Game` load-order reads (LMC-ID-005) in the Papyrus VM. Revision 45 resolves every scanned script call against that catalog in the
preflight (LMC-SCR-005). Revision 44 builds the per-function extender catalog with introduced-in
versions and measured coverage (LMC-SCR-004). Revision 43 reads every plugin as the profile's game (LMC-REC-009) and
reports light and medium records past their object space (LMC-ID-008). Revision 42 builds LMC-ID-010: a duplicate plugin
or root is a repeat or an identity collision by content. Revision 41 serves Fallout 4's MCM (LMC-UI-002 slice 5): menus by their
config folders, F4SE's external events. Revision 40 records that no `.xwm` decodes (LMC-AST-009, catalog
A7.xwma). Revision 39 serves MCM Helper's hotkeys (LMC-UI-002 slice 4b). Revision 38 measures LMC-PERF-001: a 1,000-plugin profile with
paired archives activates in 0.35 s, and archive pairing is linear. Revision 37 serves MCM Helper's config menus (LMC-UI-002 slice 4), with
the player's settings kept per named profile (LMC-SET-004). Revision 36 builds LMC-PERF-003: an asset lookup is one index
probe whatever the archive count, and opening archives is linear. Revision 35 makes SkyUI's `ShowMessage` (and any box wrapper called
for its value) wait for its answer, and resets a quest's scripts as it starts. Revision 34 builds the MCM host's native Mod Configuration page and
its adapter (slice 3). Revision 33 builds the MCM host's model and engine `SKI_ConfigBase`
(slice 2). Revision 32 designs the MCM host (LMC-UI-002) and builds its first
slice, quest-script `OnInit`. Revision 31 makes a save made in a removed plugin's cell or worldspace
load: Oblivion's safe cell, else the session's location (owner's rule). Revision 30 measures LMC-PERF-001's and -002's fold cost
and removes its two quadratic steps. Revision 29 measures Phase 7's profile-change saves on real data
(`lmc-profile-change-save.sh`) and makes the conformance runner's rule 4 measurable. Revision 28 builds LMC-REC-010: the sub-records no
parser decodes are kept for every plugin after the first. Revision 27 switches the live load to the session's own plugins with
LMC-SAV-002/003/004's consequences in its report (Phase 7 slice 3). Revision 26 makes the rest of the post-reload save
state's FormIDs stable (Phase 7 slice 2). Revision 25 saves the actor columns' FormIDs as stable
identities (Phase 7 slice 1). Revision 24 merges mods' translation files (U4)
and records why Phase 7 starts with stable saved FormIDs. Revision 23 layers mods' UI files over the vanilla
archives (#7336). Revision 22 measures LMC-AST-005 and
builds what it found missing. Revision 21 measures LMC-SET-002 and
fixes the two hard-coded GMSTs it found. Revision 20 builds the first slice of the
preflight (LMC-DX-001) and measures LMC-REC-011. Revision 19 builds LMC-REC-004 for the
cell index's records. Revision 18 records LMC-REC-007 as built
and names a no-op override's owner in the report. Revision 17 builds the merge-ownership
half of LMC-REC-006. Revision 13 builds the profile
fingerprint and its record in saves (LMC-ID-007, LMC-SAV-001). Revision 12 builds mod roots and their
ownership (LMC-ID-012), root-ordered loose files (LMC-AST-010) and the
root half of missing-master disabling (LMC-ID-011). Revision 11 extends the list merge
(LMC-REC-003) to the keyword lists of activators, flora, furniture and
containers, and records that locations and quests wait on partial forms;
revision 10 extended it to actors, races and carried items; revision 9 to
form lists, outfits and containers; revision 8 built it for levelled lists. Revision 7 records the owner's ruling
that `.esm` and `.esl` plugins pair with archives (LMC-AST-002). Revision 6 starts Phase 0: the mechanism
catalog is machine-readable and §3.1 is generated from it. Revision 5 files
the tracking issues.
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

### 3.1 Mechanism catalog

The catalog is machine-readable and lives in
[`assets/legacy_mod_compat/mechanisms.toml`](../../assets/legacy_mod_compat/mechanisms.toml)
(LMC-CLS-006). The tables below are **generated** from it by
`scripts/check-lmc-catalog.py --write` and checked by CI; they are not
edited by hand, and the hand-written "Today" column of the first draft is
gone (the state of an implementation belongs in the ROADMAP, not here).
Each row is one mechanism with exactly one disposition per game it exists
for (LMC-CLS-001); a mechanism whose parts differ (the extender binary and
the facilities it provides) is split into component rows (`N1.facilities`,
`N1.binary`). The disposition is the *current* one; `→` marks the one a
mechanism is promoted to. A change to a disposition bumps the row's version
and its citation (LMC-CLS-005). Mod names are examples of the mechanism, not
a compatibility claim about those mods. "Open Phase 0 questions" lists what
was not confirmed from a primary source; a row that has one is a starting
position, not a confirmed one.

<!-- lmc-catalog:begin -->
<!-- generated by scripts/check-lmc-catalog.py from assets/legacy_mod_compat/mechanisms.toml (catalog_version 2); do not edit -->

Games: Obl = oblivion, FO3 = fo3, FNV = fnv, Sky = skyrim, FO4 = fo4, SF = starfield. A game not listed for a row has no such mechanism.

**Content: records**

| Id | Mechanism | Examples | Disp. | Games | Reason and citation | v |
|---|---|---|---|---|---|---|
| R1 | New records in a plugin (ESM/ESP/ESL/ESH) | every content mod | D1 | all | A plugin's new records are ordinary input to the multi-plugin fold. (§5.1 LMC-ID-003) | 1 |
| R2 | Record overrides of a master's records | patches, overhauls | D1 | all | Last-in-order wins at the fold (the legacy policy); under the default merge policy independent overrides of a levelled list, form list, outfit, container, actor, race, carried item or base object's keyword list combine per list entry form and per field (LMC-REC-003). Deletions are carried. Partial forms (record flag 0x4000, Fallout 4 and Starfield) are not overlaid on the record they override yet: they replace it whole. (§5.2 LMC-REC-001; §5.2 LMC-REC-003; [`crates/plugin/src/esm/records/list_merge.rs`](../../crates/plugin/src/esm/records/list_merge.rs)) | 5 |
| R3 | Deleted / disabled records (DELE flag, UDR tombstones) | cleaned plugins, cell edits | D1 | all | Tombstones are decoded; the engine honours the deletion. (§3.1 R3) | 1 |
| R4 | Injected records (a plugin defines a form in a master's ID space) | AWKCR-style, Bashed Patch | D1 | all | Loaded as any record the plugin defines. (§3.1 R4) | 1 |
| R5 | Cell / worldspace overrides, new worldspaces, REFR edits | new lands, cell overhauls | D1 | all | The cell walker runs per plugin and the fold merges the overrides. (§3.1 R5) | 1 |
| R6 | Navmesh overrides | any cell edit | D1 | all | NAVM is decoded as data; runtime pathfinding is a separate design. ([navmesh-pathfinding.md](../engine/navmesh-pathfinding.md)) | 1 |
| R7 | Localised string tables (STRINGS / DLSTRINGS / ILSTRINGS) | Skyrim+ localised plugins | D1 | Sky, FO4, SF | String tables are read per plugin. (§3.1 R7) | 1 |
| R8 | Plugin header / form versions beyond the vanilla build | SE 1.71-header plugins, FO4 next-gen | D1 | all | The parser accepts the per-game header versions; plugins that only backport header support are redundant (see T3). (§3.1 R8) | 1 |
| R9 | Small / light flag (FExxxyyy, 4,095 records) and Starfield medium flag (FDxxyyyy, 65,535 records) | ESPFE, Creation Kit CC, Starfield medium plugins | D1 | Sky, FO4, SF | Both ID spaces are covered by the virtual legacy index (LMC-ID-005). ([blog.ortham.net](https://blog.ortham.net/posts/2024-06-28-load-order-in-starfield/); §5.1 LMC-ID-005) | 1 |
| R10 | GMST overrides from plugins | tweak mods | D1 | all | Plugin GMSTs are records like any other; consumers that hard-code authored values break this class (#6567 is one instance). (§3.1 R10) | 1 |
| R11 | SEQ files (seq/<plugin>.seq) listing Start Game Enabled quests | any Skyrim+ quest mod | D2 | Sky, FO4, SF | The engine starts Start Game Enabled quests from the QUST record itself, so the file is never read; a shipped one is ignored with a note. ([ck.uesp.net](https://ck.uesp.net/wiki/Quest_Data_Tab); [wiki.beyondskyrim.org](https://wiki.beyondskyrim.org/wiki/Arcane_University:Seq_Guide); §5.10 LMC-SAV-003) | 1 |

**Content: assets**

| Id | Mechanism | Examples | Disp. | Games | Reason and citation | v |
|---|---|---|---|---|---|---|
| A1 | Loose files under Data\ shadowing archives | texture packs, mesh fixes | D1 | all | Loose files outrank every archive. With mod roots they layer in root order, the later root winning a path (LMC-AST-010). (§5.3 LMC-AST-001; §5.3 LMC-AST-010; #6200; byroredux/src/mod_roots.rs) | 2 |
| A2 | Plugin-paired archives (per-game naming rules) | most archived mods | D1 | all | The engine opens the archives the game would pair with a plugin by name, in profile order. (§5.3 LMC-AST-002; [geck.uesp.net](https://geck.uesp.net/wiki/BSA_Files); [falloutck.uesp.net](https://falloutck.uesp.net/wiki/Archive_File); [`crates/plugin/src/archive_pairing.rs`](../../crates/plugin/src/archive_pairing.rs)) | 2 |
| A3 | Archive list order (sResourceArchiveList, sArchiveList, .ccc) | official + CC content | D1 | all | Order semantics are imported into the profile and reproduced by profile order; the files are not read at runtime. (§5.3 LMC-AST-003) | 1 |
| A4 | Archive invalidation (bInvalidateOlderFiles, redirection archives) | FO3/FNV/Oblivion texture mods | D2 | Obl, FO3, FNV | Loose files always win in this engine, so the INI flag and the redirection archive are ignored with a note. ([geck.uesp.net](https://geck.uesp.net/wiki/BSA_Files); [en.uesp.net](https://en.uesp.net/wiki/Oblivion_Mod:BSAs_and_Archive_Invalidation); §5.3 LMC-AST-004) | 1 |
| A5 | DDS textures from mod tooling | all texture mods | D1 | all | Formats are decoded in crates/renderer/src/vulkan/dds.rs; the mod corpus gates the rest. (§3.1 A5) | 1 |
| A6 | NIF meshes from mod tooling (NifSkope, Blender, 3ds Max exporters) | all mesh mods | D1 | all | The NIF parser is the input; mod NIFs join the parse corpus. (§3.1 A6) | 1 |
| A7 | Sound / voice / lip (.wav, .mp3, .ogg, .fuz, .lip) | voiced mods | D1 | all | M44 audio plays .wav, .ogg and .mp3 (symphonia); a .fuz voice line decodes its xWMA through the host's ffmpeg (#5912); the .fuz header bug is tracked as #6480. xWMA effects and music are A7.xwma. (§3.1 A7; #6480; #5912) | 2 |
| A7.xwma | xWMA effects and music (.xwm) | Fallout 4 and Skyrim sound and music mods | D5 → D1 | Sky, FO4 | unsupported: the effect decoder (symphonia: wav, ogg, mp3, flac) has no xWMA codec and music marks .xwm unresolved; the vanilla archives ship 6,463 xWMA effects of Fallout 4's 8,410 sounds and all of Skyrim's (269) and Fallout 4's (459) music, so they are silent too. Tracked as the xWMA decoder PRD the owner called for (2026-11-13, game-music MUS-NG-001). ([game-music.md MUS-NG-001](../prd/game-music.md MUS-NG-001); [p2-combat-anim-sound-fixture.md](../engine/p2-combat-anim-sound-fixture.md); LMC-AST-009) | 1 |
| A8 | SpeedTree .spt replacements | Oblivion/FO3/FNV tree mods | D1 | Obl, FO3, FNV | crates/spt reads them and imports billboards. (§3.1 A8) | 1 |
| A9 | FaceGen data (facegendata/) overrides | NPC overhauls | D1 | all | M41 FaceGen reads the data. (§3.1 A9) | 1 |
| A10.assets | LOD-generator outputs: generated plugins, object / tree / grass LOD meshes and textures, TexGen output, xLODGen terrain LOD | DynDOLOD, TexGen, xLODGen | D1 | Sky, FO4 | The .bto / .btr ladder (M35) consumes generated static LOD like vanilla LOD, until engine-side LOD generation (M53) makes it redundant. ([dyndolod.info](https://dyndolod.info/Help/Generation-Instructions); §3.1 A10) | 1 |
| A10.occlusion-dll | DynDOLOD Occlusion.esp and the DynDOLOD DLL | DynDOLOD | D2 | Sky | The engine has no occlusion planes to feed and owns dynamic LOD; the DLL is never loaded. ([dyndolod.info](https://dyndolod.info/Help/DynDOLOD-DLL); §3.1 A10) | 1 |
| A11 | Grass caches generated by No Grass In Objects (Data\Grass\ files) | NGIO, Grass Cache Helper NG | D3 | Sky | The engine's ground cover is engine-authored, so there is nothing for a precomputed cache to feed. ([github.com](https://github.com/DwemerEngineer/No-Grass-In-Objects-NG); [exal-groundcover.md](../engine/exal-groundcover.md)) | 1 |

**Content: scripts**

| Id | Mechanism | Examples | Disp. | Games | Reason and citation | v |
|---|---|---|---|---|---|---|
| S1 | Papyrus .pex (new scripts, overriding vanilla scripts) | Skyrim+/FO4/SF mods | D1 | Sky, FO4, SF | Lowering plus the bounded M47.2 VM run them; per-game rates are in ROADMAP. (§5.4 LMC-SCR-001) | 1 |
| S2 | ObScript in SCPT records | Oblivion/FO3/FNV mods | D1 | Obl, FO3, FNV | The SCDA interpreter (M47.3) runs them. (§3.1 S2) | 1 |
| S3 | Extender language extensions in ObScript (NVSE/OBSE let/eval, arrays, string vars, user-defined functions) | JIP-era FNV mods, OBSE mods | D5 → D1 | Obl, FNV | Not modelled; tracked per construct and promoted to D1 construct by construct. (§5.4 LMC-SCR-008) | 1 |
| S4.covered | Extender script functions the catalog covers (SKSE/F4SE/xNVSE/JIP/OBSE calls visible from Papyrus/ObScript) | nearly every scripted mod post-2012 | D1 | all | A function with a Native or Mapped catalog entry runs. (§5.4 LMC-SCR-004; [`crates/sdk/src/compatibility/`](../../crates/sdk/src/compatibility/)) | 1 |
| S4.uncovered | Extender script functions the catalog does not cover | nearly every scripted mod post-2012 | D5 | all | Recognised, no route yet; each call produces an attributed diagnostic (LMC-SCR-003). (§5.4 LMC-SCR-003) | 1 |
| S5 | Script-library DLLs' Papyrus API (PapyrusUtil StorageUtil, JContainers, powerofthree's Papyrus Extender, ConsoleUtil, UIExtensions) | frameworks | D4 | Sky, FO4, SF | Served by engine-owned compatibility services; file-backed, object-scoped and JDB storage are unsupported until Phase 3. (§5.5 LMC-EXT-003) | 1 |
| S6 | NVSE script runner .txt scripts, OBSE plugin INIs | FNV/Oblivion tweak mods | D5 | Obl, FNV | Not modelled. (§3.1 S6) | 1 |
| S7 | VMAD / quest fragments / scene fragments from mods | quest mods | D1 | Sky, FO4, SF | Same attach path as vanilla. (§3.1 S7) | 1 |
| S8 | Papyrus VM tuning INIs (fPostLoadUpdateTimeMS, iMaxAllocatedMemoryBytes, ...) | performance guides | D2 | Sky, FO4, SF | The engine runs no original VM to tune. (§3.1 S8) | 1 |

**Native code**

| Id | Mechanism | Examples | Disp. | Games | Reason and citation | v |
|---|---|---|---|---|---|---|
| N1.facilities | Extender runtime binaries: the facilities they provide (SKSE/F4SE/xNVSE/OBSE/SFSE) | - | D2 | all | The facilities are SDK services; nothing of the binary is needed. (§5.5 LMC-EXT-001) | 1 |
| N1.binary | Extender runtime binaries: the executable itself | - | D3 | all | It patches the original executable and is never loaded. (§5.5 LMC-EXT-001) | 1 |
| N2 | Engine-fix plugins | SSE Engine Fixes, Bug Fixes SSE, Scrambled Bugs, NVAC, NVTF, Buffout 4, Oblivion engine fixes | D2 | all | Each fix is re-derived into the built-in fix catalog. ([builtin-fixes.md](../engine/builtin-fixes.md); §5.5 LMC-EXT-002) | 1 |
| N3 | Memory / heap / LAA / crash-logger / address-library / preloader plugins | 4GB patch, SSE Display Tweaks' allocator half, Address Library | D3 | all | The engine is 64-bit Rust; there is nothing to port. (§3.1 N3) | 1 |
| N4 | Gameplay plugins with engine hooks | Precision, TK Dodge RE, Place Everywhere, Better Jumping | D4 | all | Valid gameplay semantics behind native code; supported by source port to the SDK. ([sandboxed-linked-mods.md](../engine/sandboxed-linked-mods.md#16-c-and-c-sdk-requirements); §5.5 LMC-EXT-004) | 1 |
| N5 | Renderer injectors | ENB, ReShade, Community Shaders, Oblivion Reloaded | D3 | all | A different renderer by design. (§3.1 N5) | 1 |
| N6 | Behaviour-graph generators' outputs (behaviors/*.hkx, animations/*.hkx, animationdatasinglefile.txt) | FNIS, Nemesis, Pandora | D1 | Sky, FO4 | The outputs are assets; crates/hkx reads Skyrim and FO4 graphs. The modded-graph runtime is unverified. (§5.9 LMC-ANIM-001) | 1 |
| N7 | Conditional animation replacement conventions (DAR _CustomConditions, OAR config.json) | animation mods | D4 | Sky | Read natively by the engine (engine-native service), no DLL. (§5.9 LMC-ANIM-002) | 1 |
| N8 | Head / face asset overrides needing a DLL (Blockhead) | OCOv2 | D4 | Obl | Replaced by a profile-level override table keyed by stable form identity (engine-native). (§5.9 LMC-ANIM-005) | 1 |
| N9 | Character-editor plugins (RaceMenu, LooksMenu) and the body-morph runtime (.tri) | body/face mods | D5 | Sky, FO4 | Runtime morph updates are not built. (§5.9 LMC-ANIM-003) | 1 |

**UI**

| Id | Mechanism | Examples | Disp. | Games | Reason and citation | v |
|---|---|---|---|---|---|---|
| U1 | Scaleform menu replacement .swf | SkyUI, DEF_UI, HUDFramework, StarUI | D1 | Sky, FO4, SF | A mod's movie resolves over the vanilla interface archive through every mod tier: loose interface files per root, then plugin-paired archives (#7336). Its host methods are served within the M48 catalogs; an unmapped method is an attributed diagnostic. (§5.6; byroredux/src/asset_provider/layered.rs) | 2 |
| U2 | Mod Configuration Menus (SkyUI MCM, MCM Helper, FO4 MCM, FNV MCM via UIO) | most configurable mods | D4 | all | Served by an engine-owned MCM host. (§5.6 LMC-UI-002) | 1 |
| U3 | MenuXml menu replacements and prefabs | DarNified UI, Vanilla UI Plus, oHUD, UIO | D1 | Obl, FO3, FNV | crates/menuxml reads them, including <include> prefabs; a mod's menus, prefabs, fonts and HUD textures resolve over the vanilla Misc and texture archives through every mod tier (#7336). (§3.1 U3; byroredux/src/asset_provider/layered.rs) | 2 |
| U4 | Translation files (Interface/Translations/*_<lang>.txt) | SkyUI, localised mods | D1; SF D5 | Sky, FO4, SF | The script extender, not the game, merges them: SKSE64 and F4SE read Interface/Translations/<plugin stem>_<language>.txt for every enabled plugin in load order, UCS-2 LE with its mark, and keep a key's first definition (Translation.cpp ParseTranslation / ImportTranslationFiles; tHashSet::Insert). The engine does the same natively. Starfield: no source for SFSE's behaviour, so not claimed. (§3.1 U4; [`crates/ui/src/translate.rs`](../../crates/ui/src/translate.rs); [github.com](https://github.com/ianpatt/skse64/blob/master/skse64/Translation.cpp); [github.com](https://github.com/ianpatt/f4se/blob/master/f4se/Translation.cpp)) | 3 |
| U5 | Font replacements (fontconfig.txt, .swf fonts, .fnt) | UI mods | D1 | all | fontconfig.txt and fonts_*.swf resolve through the Scaleform sources, .fnt through the MenuXml sources, each layering every mod tier over the vanilla archives (#7336). (§3.1 U5; byroredux/src/asset_provider/layered.rs) | 2 |

**Settings and tooling outputs**

| Id | Mechanism | Examples | Disp. | Games | Reason and citation | v |
|---|---|---|---|---|---|---|
| T1 | Unofficial patches as plugins | UOP, UFO3P, YUP, USSEP, UFO4P, SCP | D2 | all | A known patch is not loaded; the engine stands in for dependents (decision Q1). (§5.8 LMC-FIX-001; §5.8 LMC-FIX-004) | 1 |
| T2.plugin | Generated patch plugins: the plugin | Bashed Patch, Smashed Patch, Synthesis output, Reqtificator output | D1 | all | A generated plugin loads as any plugin. (§3.1 T2) | 1 |
| T2.necessity | Generated patch plugins: the need for them where record merging covers it | Bashed Patch, Smashed Patch, Synthesis output | D2 | all | The per-form merge policy covers the case the patch exists for: levelled lists, form lists, outfits, containers, the lists of actors and races, and the keywords of items, activators, flora, furniture and containers today; locations and quests are still last-wins until partial forms are overlaid. (§5.2 LMC-REC-003; [`crates/plugin/src/esm/records/list_merge.rs`](../../crates/plugin/src/esm/records/list_merge.rs)) | 5 |
| T3.plugin | Merged plugins, ESL-flagged plugins, 'cleaned' masters: the plugin | load-order management | D1 | all | Loads as any plugin; there is no slot ceiling in the design. (§5.1 LMC-ID-004) | 1 |
| T3.necessity | Merged plugins, ESL-flagged plugins, 'cleaned' masters: the need for them | load-order management | D2 | all | Plugin-count workarounds are not needed. (§5.1 LMC-ID-004) | 1 |
| T4.data | FO4 precombine / previs repair packs: the data | PRP | D1 | FO4 | Loads as data; the engine falls back to REFRs when precombines are absent (M49). (§5.2 LMC-REC-012) | 1 |
| T4.necessity | FO4 precombine / previs repair packs: the need for them | PRP | D2 | FO4 | Forced REFR-first on override covers the case. (§5.2 LMC-REC-012) | 1 |
| T5 | Game INI edits for removed limits (uGridsToLoad, heap, bInvalidateOlderFiles, Papyrus) | performance guides | D2 | all | The limits do not exist in this engine; the INI is not read. (§5.7 LMC-SET-001) | 1 |
| T6 | Game INI edits for graphics | ENB guides, Ultra.ini tweaks | D3 | all | The engine settings registry owns graphics. (§5.7 LMC-SET-001) | 1 |
| T7.registered | Game INI edits for gameplay-visible values the settings registry has | FOV, difficulty, fXP... | D1 | all | An honoured key maps to a registry entry (the honoured-INI table, LMC-SET-001). (§5.7 LMC-SET-001) | 1 |
| T7.other | Game INI edits for gameplay-visible values the settings registry does not have | difficulty tweaks, fXP... | D5 | all | Reported with a note, tracked. (§5.7 LMC-SET-001) | 1 |
| T8 | Mod-provided INIs / JSON (Data/SKSE/Plugins/*.ini, MCM/Settings/*.json, NVSE/Plugins/*.ini) | configurable DLL mods | D4 | all | Readable by a ported extension through scoped content read (SEC-018). ([sandboxed-linked-mods.md](../engine/sandboxed-linked-mods.md#102-requirements); §5.7 LMC-SET-003) | 1 |
| T9 | Cosaves (.skse, .f4se, .nvse) and save editors | ReSaver, FallrimTools | D3 | all | There is no legacy save format (ROADMAP anti-scope). (§3.1 T9) | 1 |
| T10.vfs | Mod-manager virtual filesystems | MO2 USVFS | D3 | all | The engine reads a profile, not a virtual filesystem: each mod directory is a mod root (--mod-root, in priority order) whose plugins resolve by name and whose loose files layer in root order, as the virtual filesystem presents them (LMC-ID-012, LMC-AST-010). (§5.1 LMC-ID-006; §5.1 LMC-ID-012; byroredux/src/mod_roots.rs) | 2 |
| T10.outputs | Mod-manager deployment layouts and profiles | Vortex, MO2 profiles | D1 | all | Imported once into a profile: --import-load-order reads plugins.txt / loadorder.txt, an MO2 profile (modlist.txt priority, its enabled mod directories as roots) or a Vortex deployment (staged mods as roots, ordered so each deployed file's mod wins) and writes the boot request's [mods]. A deployed mod directory can also be given as a mod root directly (--mod-root, LMC-ID-012). (§5.1 LMC-ID-006; §5.1 LMC-ID-012; [`crates/plugin/src/load_order_import.rs`](../../crates/plugin/src/load_order_import.rs); byroredux/src/load_order_import.rs; byroredux/src/mod_roots.rs) | 3 |
| T11.plugin-enablers | Plugin-limit and plugins.txt enablers | Plugins.txt Enabler | D2 | SF | The engine has no plugin gate to open. (§3.1 T11) | 1 |
| T11.achievement-enablers | Achievement enablers | Achievement Enabler | D3 | SF | Targets the original executable's achievement lock. (§3.1 T11) | 1 |
| T12 | Total conversions | Tale of Two Wastelands, Enderal | D1 | all | Content loads as content; not attempted, corpus tier is decision Q6. (§10 Q6) | 1 |

**Open Phase 0 questions** (a row with one is a starting position, not a confirmed one):

- R3: REFR semantics of DELE / UDR tombstones
- R4: behaviour of injected records, from a primary source
- R5: cell / worldspace override merge, per field
- R11: every load path starts Start Game Enabled quests without a SEQ file
- R11: the SEQ requirement on Fallout 4 and Starfield (the cited CK page states it for Skyrim 1.7+)
- A2: Skyrim LE / SE: the paired name '<stem> - Textures.bsa' (only '<stem>.bsa' is confirmed, by the Creation Kit wiki)
- A2: FO4: how the high-resolution texture pack's 'DLCUltraHighResolution - Textures01'..'16' archives are opened (no '- Textures' archive heads the series)
- A9: override of FaceGen data by loose path
- S4.uncovered: per-function 'introduced in' API level of every extender and library family, from changelogs
- S5: probe functions of MCM Helper, Fallout 4 MCM, ConsoleUtil, UIExtensions, HUDFramework, Workshop Framework and SFSE
- U5: font-replacement handling
- T7.registered: the honoured-INI table: INI key to registry entry per game, each key from a primary source

<!-- lmc-catalog:end -->

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
  Built ahead of Phase 2 (#6670): `byroredux --game <key> --import-load-order
  <source> [--write <boot.toml>]` imports a `plugins.txt`, a `loadorder.txt`,
  an MO2 profile or a Vortex deployment (repeatable, so a Vortex deployment
  takes the game's `plugins.txt` beside it), prints the active plugins in
  load order with the root each comes from, the roots lowest priority first
  and a note per unresolved entry, and writes the launcher boot request's
  `[mods]` table, which `--boot` expands into `--master` / `--esm` /
  `--mod-root`. The rules are cited from the tools that write these files
  (`crates/plugin/src/load_order_import.rs`): libloadorder for the per-game
  methods (file time on Oblivion, Fallout 3 and New Vegas; `loadorder.txt`
  on Skyrim LE; asterisk `plugins.txt` on Skyrim SE, Fallout 4 and
  Starfield), the early-loading and `.ccc` plugins and New Vegas `.nam`
  activation; MO2's sources for `modlist.txt` priority, separators, foreign
  entries, `overwrite` and the instance directories; a live Vortex
  deployment for its manifest, whose staged mods are ordered so each
  deployed file's mod wins. A Vortex mod whose staging folder is gone stays
  in `Data`, unowned, with a note. MO2's `archives.txt` order is reported,
  not reproduced. Not modelled: `sTestFile`, Starfield's `My Games` `.ccc`
  and `BlueprintShips-` activation, ghosted plugins.
- **LMC-ID-007.** A profile MUST carry a fingerprint over plugin identities,
  order, content hashes, extension packages, grants and policies
  ([PKG-010](../engine/sandboxed-linked-mods.md#8-package-manifest-requirements),
  [STATE-011](../engine/sandboxed-linked-mods.md#13-persistent-state-and-saves)).
  Saves store it (§5.10). Built for the session profile (#6670):
  `byroredux/src/profile_fingerprint.rs`. The record holds the plugins in
  load order by identity, each with the xxHash64 of its file (hashed in the
  load-order fold), the mod roots in order, each extension manifest with
  its hash, the grants and the record policy; the fingerprint is the
  xxHash64 of all of it. Extension component hashes and selected providers
  wait for the package manifest (PKG-010); a root's loose files enter by
  root identity and order, not content.
- **LMC-ID-008.** ESL / ESH / ESM header flags affect only Form ID decoding
  and masters-first ordering. "Small" MUST NOT impose a form count beyond
  the 12-bit object space the file was authored in (4,095 records), and
  "medium" MUST NOT impose one beyond its 16-bit object space (65,535
  records) ([Ortham](https://blog.ortham.net/posts/2024-06-28-load-order-in-starfield/)).
  The original engines' active-plugin ceilings for each class do not exist
  here (LMC-ID-004).
  At HEAD on 2026-11-23 (#6670) the flags decide only a plugin's slot, read
  as the profile's game (LMC-REC-009), and so its Form ID decoding; no
  record count is imposed. A light or medium plugin's own records whose
  object ids its space cannot hold are masked into it, as the slot decodes
  them, and the fold logs how many (`own_records_outside_object_space`,
  `byroredux/src/cell_loader/load_order.rs`). The per-class slot ceilings
  that remain are LMC-ID-004's.
- **LMC-ID-009.** Plugin names, archive names, asset paths and script names
  MUST resolve case-insensitively, with `\` and `/` equivalent, on every
  host filesystem.
- **LMC-ID-010.** Two enabled packages with the same identity and different
  content MUST be an activation diagnostic, never a silent replacement
  ([PKG-013](../engine/sandboxed-linked-mods.md#8-package-manifest-requirements)).
  Built for the session profile (#6670). A plugin whose identity an earlier
  profile entry already has is left out and compared byte for byte with
  that entry (`same_file_content` in `crates/plugin/src/activation.rs`):
  the same file or the same bytes is reported as a repeat, different bytes
  as an identity collision naming both paths; the earlier entry loads
  either way. A mod root's directory listed twice is one root; two
  different directories with one identity are reported as an identity
  collision without comparing their files, and both load, the later
  winning their loose files (`byroredux/src/mod_roots.rs`). Extension
  packages wait for the package manifest's content hashes (PKG-010).
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
  that session, never silently omitted. The root half is built with
  LMC-ID-012: a root whose every profile plugin is disabled loses its loose
  files for the session, and one that keeps a plugin keeps them with a
  warning.
- **LMC-ID-012 — Ownership.** The profile MUST record, for every mod root,
  its package identity and the plugins, archives and loose files it ships;
  content found directly in the game's `Data` directory belongs to an
  implicit "unmanaged" root that sorts first. For a root without a
  manifest, the import derives the identity from the root's directory name,
  case-folded. Ownership is what lets LMC-ID-011 disable a whole mod,
  LMC-DX-005 attribute a diagnostic to the root whose file actually won,
  and LMC-SCR-010 answer probes per mod from the scripts that actually run.
  LMC-ID-006 populates it on import.
  Built for the session profile (#6670): `byroredux/src/mod_roots.rs`. The
  profile names its roots with `--mod-root <dir>`, repeated, lowest
  priority first, after the unmanaged root, the base master's directory.
  Each root owns the plugins and archives at its top level and the loose
  files under it; its identity is its directory name, case-folded, and two
  directories sharing one are reported (LMC-ID-010). A plugin named by its bare
  file name resolves through the roots, the highest-priority root first and
  the unmanaged root last. `plugins.activation` and `--plugin-report` list
  each root with its plugins, archives and loose files. Manifest identities
  are not built; the import that writes roots is (LMC-ID-006).

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
  Built for the levelled lists (`LVLI`, `LVLN`, `LVLC`, `LVSP`), form
  lists (`FLST`), outfits (`OTFT`), containers (`CONT`), actors (`NPC_`,
  `CREA`: inventory, factions, spells, perks, keywords, actor values),
  races (`RACE`: spells, keywords, race relations) and carried items
  (`ALCH`, `AMMO`, `ARMO`, `BOOK`, `MISC`, `WEAP` and the other item types:
  keywords) and the keyword lists of activator, flora, furniture and
  container bases (`ACTI`, `FLOR`, `FURN`, `CONT`; kept apart from their
  records, they merge on their own), #6670:
  `crates/plugin/src/esm/records/list_merge.rs`, selected per session with
  `--merge-policy merge|legacy` (default `merge`). "Independent" is decided
  on the transitive master closure, so a patch that masters every
  contributor wins whole; an override's base is its latest earlier carrier
  among its own masters. A wholesale winner's changes against the list it
  replaced count as its operations, so a later independent override that
  undoes one overrules it in the report. Per-entry `COED` data is not
  decoded and takes no part in the comparison. A form list's null entries
  hold a place, not a form: they are never an operation and keep their
  positions, and additions append, so the indices of untouched entries stay
  put. A list whose order is its meaning is one scalar field: an actor's
  AI packages (priority order) and a consumable's effects. Locations
  (`LCTN`) and quests (`QUST`) are still last-wins: Fallout 4 and
  Starfield write overrides of both as partial forms (record flag `0x4000`,
  xEdit "Partial Form"), which carry only the sub-records they change, and
  the fold does not overlay a partial form on the record it overrides yet,
  so a merge would read every absent sub-record as a removal. The
  installed DLC and Creations carry 49 partial `LCTN`, 40 partial `CELL`,
  15 partial `WRLD` and 7 partial `QUST` records on Starfield and 19
  partial `LCTN` and 7 partial `QUST` on Fallout 4 (Sim Settlements 2 adds
  235 partial `CELL`); under either policy such an override replaces the
  record whole today. Skyrim's installed data carries none.
  Spells and magic effects decode no keyword list. On the installed games the
  vanilla master-plus-DLC sets merge no levelled list (their DLC extend
  shared lists by script); Skyrim's Dawnguard and Hearthfires both override
  three containers and merge to what `legacy` keeps. A Fallout 4 profile
  with the Unofficial Fallout 4 Patch and MojaveImports merges two levelled
  lists, where `legacy` let the mod built against vanilla undo the patch's
  fix; a Starfield profile with the community, unofficial and Resource
  Revival patches merges a container whose rename and item swap `legacy`
  lets the vanilla-based unofficial patch undo. With Inventory Organizer
  added, the Fallout 4 profile merges 568 items: `legacy` lets
  MojaveImports' record of Nuka-Cola Quantum undo Inventory Organizer's
  rename, `merge` keeps the rename with MojaveImports' effects and keyword.
  A Starfield companion mod raises a companion's perks and actor values
  and adds two perks; `legacy` lets the vanilla-based unofficial patch
  undo all of it, `merge` keeps it beside the patches' own changes.
- **LMC-REC-004.** DELE-flagged overrides and tombstoned references MUST
  behave as the game treats them: a deleted base record disappears from the
  merged index; a deleted reference is absent from its cell. The cleaning
  convention of "undeleting then disabling the references and moving it to
  a position where it is no longer visible"
  ([Tome of xEdit §7](https://tes5edit.github.io/docs/7-mod-cleaning-and-error-checking.html))
  produces data the engine already handles as a disabled, moved reference.
  Built (#6670): every map of the merged index prunes a deleted form,
  including the cell index's cells, modelled base objects (`statics`),
  pack-ins and material forms, which kept theirs until revision 19 (Skyrim
  SE's deleted statics `0006CD7C` and `000BD6A5`, Fallout 4 DLCCoast's
  interior cell `001DFB07` and pack-in `001DFB9F`). Measured against a raw
  record walk on FNV, Skyrim SE and Fallout 4 with UFO4P: no deletion that
  ends its form's chain survives the fold. Two consequences have no
  documented original-engine behaviour. **Phase 0: verify** what the game
  does with a live reference whose base a later plugin deletes (Skyrim.esm's
  `0010C5CD` in Bthardamz03 places Dawnguard's deleted
  `DA13VineCeiling01NoCol`; the engine skips it, as it skips any reference
  without a base). **Phase 0: verify** what it does with references a
  deleted CELL still holds that no deletion covers (none in the measured
  data; the engine drops them with the cell and logs them).
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
  Built (#6670): the conflict report's verdicts (`plugins.conflicts <form>
  fields`) judge each override against the record it replaced, and under
  `merge` an override identical to the record it would replace is a no-op
  that neither wins the record whole nor becomes a contributor. An
  independent override identical to its own master changes nothing under
  `merge`; the report names it, since under `legacy` it wins whole and
  reverts the changes before it. On the installed mod profiles this takes
  the merged-record count from 570 to 284 (Fallout 4 with UFO4P, Inventory
  Organizer and MojaveImports) and from 1,728 to 415 (Starfield with the
  patches and both weightless-item mods); the overruled operations are
  unchanged. The records that dropped out are those whose outcome is what
  `legacy` keeps.
- **LMC-REC-007.** The conflict report lists, per form: winner, losers in
  order, the policy that chose, and the fields that differ. This is the
  `DataStore` provenance model ([plugin-loading.md](../engine/plugin-loading.md#datastore))
  applied at the live fold.
  Built (#6670): `plugins.conflicts [formid] [fields]` and `--plugin-report`
  ([plugin-loading.md](../engine/plugin-loading.md) item 4). The winner is
  the carrier that owns the record, so an override the merge judged a no-op
  (LMC-REC-006) is marked as one, not named the winner. The merge reads
  decoded fields and `fields` compares bytes; when they disagree the line
  says so. Gun Runners' Arsenal's frag grenade (WEAP `00004330`) is such a
  case: it differs from FalloutNV's only in DNAM's Attack Shots/Sec, which
  no decoder reads (xEdit `wbDefinitionsFNV.pas`, WEAP DNAM offset 88).
- **LMC-REC-008.** Built-in record errata apply after official content and
  before the first user plugin ([builtin-fixes.md §5.1](../engine/builtin-fixes.md#51-record-errata-after-official-content-before-user-plugins)).
  The fold loop MUST learn the official/user boundary; it does not have one.
- **LMC-REC-009.** A plugin's game dialect comes from its header and the
  profile's game, never from consumer branches
  ([per-game translation rule](../engine/nif-engine-translation-layer.md)).
  Built (#6670): the base master names the profile's game (`ProfileGame`,
  `crates/plugin/src/esm/reader.rs`), and the fold reads every plugin's
  record layouts and TES4 light and medium bits as that game, as xEdit tests
  a file's light bit against the game it loads (`TwbMainRecordStructFlags.IsLight`:
  `gcLightPlugins in aGameDef.Capabilities` and the game's `LightFlag`). An
  ESL-flagged plugin with an older header is light in a Skyrim SE profile; a
  plugin whose header bands as another game is read as the profile's game
  and logged. The header still gives the plugin's own HEDR, which picks its
  character rules (Fallout 3 or New Vegas). The preflight's ObScript scan
  reads plugins the same way.
- **LMC-REC-010.** Sub-records the parser does not decode MUST be preserved
  as opaque bytes with provenance, so a later decoder sees them and the
  explain tooling can show that a mod touched them
  ([ARC-010](../engine/sandboxed-linked-mods.md#71-package-content-and-attachment)).
  Built (#6670): no parser records what it reads, so "not decoded" is
  measured: for each record type of each game's vanilla plugins, a sampled
  copy of the plugin is parsed with each sub-record removed in turn, and a
  tag whose removal leaves the index unchanged is not decoded (6 games, 789
  (game, record type) entries, checked in as `crates/plugin/data/decoded_subrecords.toml`,
  re-derived by ignored tests). The reader keeps the undecoded sub-records
  of every record it reads, and every record or group no parser reads, of
  every plugin but the first, stamped with the plugin that carried them;
  `plugins.conflicts <form>` names them. The first plugin is the game's own
  master, standing in for the official/user boundary that LMC-REC-008 has
  yet to define; the owner chose to hold the mods' bytes and leave the
  official ones on disk. A mod plugin holds about a quarter of its file
  (the Unofficial Fallout 4 Patch: 14.6 MB, 3.5 MB held).
- **LMC-REC-011.** Cell overrides merge per cell, with the winning plugin's
  placed references replacing the loser's for the same reference ID and new
  references appended; navmesh overrides take the winner's navmesh for that
  cell. Navmesh island reconciliation is not required by this document.
  Measured 2026-11-23 against an independent raw-record reader on FO3, FNV,
  Skyrim SE and a Fallout 4 + Sim Settlements 2 profile; the gate is
  `docs/smoke-tests/lmc-fold-conformance.sh`.
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
  profile order, whatever the plugin's extension: `.esm` and `.esl` plugins
  pair exactly as `.esp` plugins do (owner ruling on #6670, 2026-11-22). The
  sources below speak of `.esp` files, but the shipped data pairs masters
  too: every FO3 / FNV DLC `.esm` ships `<Stem> - Main.bsa` (and most a
  `- Sounds.bsa`), every Skyrim SE Creation Club `.esm` / `.esl` its
  `<stem>.bsa`, and no shipped INI or `.ccc` lists any of them. The
  per-game rules, with their verification state:
  - *Oblivion* (verified): `<PluginName><anything>.bsa`, where `<anything>`
    is empty or starts with a space, hyphen or underscore
    ([UESP: BSAs and Archive Invalidation](https://en.uesp.net/wiki/Oblivion_Mod:BSAs_and_Archive_Invalidation)).
    Vanilla archives come from the INI `sArchiveList`.
  - *FO3 / FNV* (verified, Phase 0): `JunkyMod.esp` loads `JunkyMod.bsa`;
    the GECK's second method names the archive `JunkyMod - Assets.bsa`
    ("You can change Assets to whatever you like")
    ([GECK: BSA Files](https://geck.uesp.net/wiki/BSA_Files)). So the rule
    is `<stem>.bsa` or `<stem> - <anything>.bsa`; `Foo - Main.bsa` and
    `Foo - Sounds.bsa` are instances of the second form. The GECK's "the
    above method doesn't work for .esm files" is superseded by the ruling
    above.
  - *Skyrim LE / SE* (`<stem>.bsa` confirmed, Phase 0; the rest **Phase 0:
    verify**): the Creation Kit wiki's archive instructions say to "name
    your bsa the same name as your .esp file" and speak of "an archive
    already associated with your plugin" ([CK: Archive.exe](https://ck.uesp.net/wiki/Archive.exe)).
    Community sources describe `<stem> - Textures.bsa` as a second paired
    name; the Creation Kit wiki page for Bethesda archives returned 403 to
    the Phase 0 fetch and no primary source for that form was found, so it
    is not paired. Vanilla archives come from
    `sResourceArchiveList` / `sResourceArchiveList2`, "a list of BSAs to be
    loaded without a plugin"
    ([STEP: Skyrim INI](https://stepmodifications.org/wiki/Guide:Skyrim_INI/Archive)).
  - *FO4* (verified): `<stem> - Main.ba2` and `<stem> - Textures.ba2`;
    "Archive files require a Data File of the same name to be activated"
    ([FO4 CK: Archive File](https://falloutck.uesp.net/wiki/Archive_File)).
    The voice archive is `<stem> - Voices_<sLanguage>.ba2`: each voiced DLC
    ships `DLC<Name> - Voices_en.ba2`, which `Fallout4_Default.ini` does not
    list. **Phase 0: verify** how the high-resolution texture pack's
    `DLCUltraHighResolution - Textures01`..`16` open (no `- Textures`
    archive heads the series); they are not paired.
  - *Starfield* (verified): `- Main`, `- Textures`
    ([Archive2](https://starfieldwiki.net/wiki/Starfield_Mod:Archive2)),
    `- Localization` and `- Voices_<sLanguage>`. Pairing is automatic: the
    shipped `SFBGS003.esm` (Shattered Space) brings `- Main`, `- Textures`
    and five `- Voices_<lang>` archives, and `Constellation.esm` /
    `OldMars.esm` a `- Localization` archive, none of them in
    `Starfield.ini`'s archive lists. The `_Xbox` variants are console
    archives and are not paired.
  The engine has no language setting: voice archives pair in English, the
  language the profiles' voice archives already are. Numeric siblings the
  engine already pairs stay paired. Archive-versus-archive precedence: the
  game's official archive list first, then paired archives in profile
  plugin order. Loose files outrank all of them (LMC-AST-001, LMC-AST-010).
  Built for the session profile (#6670): `crates/plugin/src/archive_pairing.rs`
  holds the rules, and boot activation appends each activated plugin's
  archives from its own directory to the mesh, texture, sound, script and
  material providers, an archive the profile already lists keeping its
  place (see [Plugin Loading](../engine/plugin-loading.md)).
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
  Measured 2026-11-23 (FNV, Skyrim SE, a Fallout 4 + SS2 profile): every
  record model and texture path the load orders author resolves in the
  shapes they use (plain, `Meshes\`-rooted, `/`-separated, any case); the
  misses are content no archive ships. Built: one `strip_data_root`
  (`byroredux_core::paths`) drops leading separators and a leading `Data\`
  for meshes, textures and materials alike, and the loose index now covers
  `textures`, `meshes`, `materials`, `geometries`, `music`, `sound`,
  `scripts` and `lodsettings`, with a loose script tier ahead of every
  script archive. The UI's files resolve the same way (#7336): Scaleform
  movies and fonts over the vanilla interface archive, MenuXml menus,
  prefabs, fonts and HUD textures over the vanilla Misc and texture
  archives, each under every mod tier (`interface`, `menus` and `fonts` are
  loose roots too). A mod's own `Interface\Translations\<plugin>_<lang>.txt`
  merges into the language's table as SKSE64 and F4SE merge it (catalog U4:
  every enabled plugin in load order, UCS-2 LE, a key's first definition
  kept); Starfield's extender has no source, so U4 is not claimed there.
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
  Not met for `.xwm` (measured 2026-11-23, catalog A7.xwma, D5): the effect
  decoder (symphonia, built with wav, ogg, mp3 and flac) has no xWMA codec,
  and music marks `.xwm` unresolved (game-music MUS-NG-001). The vanilla
  archives ship 6,463 xWMA effects of Fallout 4's 8,410 sounds, and all of
  Skyrim's (269) and Fallout 4's (459) music, so those are silent along with
  any mod's `.xwm`. A `.fuz` voice line decodes its xWMA through the host's
  `ffmpeg` (#5912); `.wav`, `.ogg` and `.mp3` decode. The decoder is the
  xWMA PRD the owner called for (2026-11-13).
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
  explains every winner. Built with LMC-ID-012: every asset provider's
  loose tier is the roots' loose files in root order, above every archive,
  and the report counts each root's files that take a path from an earlier
  root.

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
  Phase 3). Built (revision 44): the per-function catalog in
  `docs/data/extenders/` (3 709 functions over SKSE64, F4SE, the NVSE plugin
  families and OBSE, each with its introducing version and its source), its
  coverage measured and published in
  [extender-catalog.md](../engine/extender-catalog.md) §5; SFSE publishes no
  script sources and has no rows.
- **LMC-SCR-005 — Eager preflight.** Preflight scans every script in the
  profile, archived and loose, not only those observed at attach time.
  Today the registry is observed-only
  ([scripting.md](../engine/scripting.md#pex-extender-compatibility-preflight)).
  Built for the catalog (revision 45): `--preflight` resolves every compiled
  Papyrus call of a root through its receiver's declared type and the game's
  script hierarchy, and every ObScript source's command names, to catalog rows,
  and reports per family the functions called, served and not, with their
  introducing versions ([extender-catalog.md](../engine/extender-catalog.md) §6).
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
  Today only `UI.IsMenuOpen` is served. Design:
  [mcm-host.md](../engine/mcm-host.md) (owner's decisions 2026-11-23: an
  engine `SKI_ConfigBase` shadowing SkyUI's, a native Mod Configuration page,
  reached from the native pause menu). Its slice 1 is built: quest scripts
  receive `OnInit` once per save (CK *OnInit*), which the registration needs.
  Slice 2 is built (revision 33): the engine's `SKI_ConfigBase` (Papyrus
  shadowing SkyUI's, SkyUI's constants and option ids), the `McmHost` model
  it writes, SkyUI's dispatch of state options, re-registration after every
  load, and the `mcm.*` console commands. `ShowMessage` works since
  revision 35: a box wrapper (a function whose body is one `Return`) called
  for its value hands its box to the calling statement, which waits and
  replays with the answer as it does for a `Message.Show()` written in place.
  A quest's scripts also reset as it starts (CK *OnInit*, revision 35).
  Slice 3 is built (revision 34): the native Mod Configuration page
  (reached from the pause menu once a mod registered) and the adapter that
  feeds it from `McmHost` and runs its actions as the config script's events.
  Slice 4 is built (revision 37): MCM Helper's `config.json` menus and its
  `MCM` API, with the player's settings per game and named profile
  (LMC-SET-004). Slice 4b is built (revision 39): its `keybinds.json`
  hotkeys, bound through its key maps and run on a key press. Slice 5 is
  built for Fallout 4 (revision 41): FO4 MCM's `config.json` menus, found by
  their folders, and F4SE's external events; New Vegas' MCM is its own slice.
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
  Measured (#6670): of the GMSTs the five shipped masters author (Oblivion
  382, Fallout 3 530, New Vegas 648, Skyrim 1,584, Fallout 4 2,039), the
  engine's non-test code reads 240 through the merged index and names 8
  without reading them. Six of those are explanation or behaviour declared
  not modelled (the three mine settings, `iCombatTargetLostRemoveSearchCount`,
  `fAVDHealthEnduranceOffset`, `fJumpHeightMin`). Two were hard-coded values,
  now read: `fSkillUseCurve` (Skyrim's skill-rank cost exponent, 1.95) and
  `fCombatDistance`, the melee reach baseline a weapon's Reach multiplies
  (GECK wiki *Weapons*). The engine used 180 for every game; Skyrim,
  Fallout 4 and Fallout 76 author 141, and Fallout 3 and New Vegas default
  to 128 (GECK wiki *FCombatDistance*). The measurement finds only
  hard-codes whose code names the setting. **Phase 0: verify** Oblivion's
  default for `fCombatDistance` (its master authors none, and no source
  gives the executable's; the engine keeps 180) and what Starfield's
  `fCombatDistance` is (its master authors 2.0, so it is not this reach, and
  the engine keeps its own there).
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
  ordered plugin list with content hashes. Built (#6670): the record is the
  save column `SessionProfile` (format v142). A load keeps the session's
  profile and reports each difference from the save's: a plugin absent,
  added or changed, a reorder, the mod roots, extensions, grants or
  policy. `save.info` prints the same.
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
  Measured 2026-11-23: the premise holds for reference identity
  (`FormIdComponent` saves the pair) but not for saved state in general.
  Inventories, equipment, actor values, perks, faction reputation,
  persistent reference states and the scripting ledgers save session-global
  `u32` FormIDs, so the live load reloads the *save's* plugin list to keep
  them meaning the same forms. Hence a removed plugin refuses the load and an
  added one is not loaded. Phase 7 therefore starts with stable saved
  FormIDs. The owner chose to save each FormID as its stable identity
  (2026-11-23). Slice 1 is built: `byroredux_core::stable_form_id`'s serde
  adapters write a saved FormID as its (plugin, object) pair through the
  session's load order and read it back through the current one, a pair
  whose plugin is gone reading as absent and counted for the load report;
  the actor columns (inventory, equipped weapon and arrow, perks, faction
  reputation, race, spells, readied spell) use them (save format v145 after
  #7348's v144). Slice 2 is built (v146): every other saved FormID the
  post-reload restore reads, including quest-keyed and pair-keyed maps,
  Papyrus registrations, the effect and parked-reference codecs,
  script-instance properties and the reference variables kept as `f32`
  halves; a guard classifies every save column as stable, FormID-free or
  pre-reload raw. Slice 3 is built (v147): the six ledgers restored before
  the reload save stable identities too, and the live load reloads the
  *session's* plugins, not the save's. A removed plugin's references are
  not spawned by the reload and their saved rows are dropped, not parked;
  inventory stacks whose base form reads as absent are emptied in place,
  their instances freed and slots released. The load report (log and HUD)
  counts, per removed plugin, its saved forms read as absent, its
  references left out and the stacks removed; it counts the references a
  plugin added since the save edits, whose saved state masks the edit, and
  those a reorder gave another winner, each with an example FormID; and the
  Start Game Enabled quests the save had not seen, which start on the next
  tick. Not yet built: STATE-008's opaque, purgeable preservation of an
  absent package's script and extension state (it is dropped with the
  forms), and a per-item list of removed stacks (a stack whose plugin is
  gone reads back as form 0, so the report counts them). Measured on real
  data (revision 29): `docs/smoke-tests/lmc-profile-change-save.sh` loads
  an FNV save across a removed, an added and two swapped generated plugins
  and checks every consequence above in the log and the world; the Oblivion
  gate `oblivion-live-load-plugin-set.sh` loads saves both ways between
  Oblivion.esm alone and Oblivion.esm with Knights.esp and
  DLCThievesDen.esp, the engine-supplied actor values moving onto the
  session's slot (80 and 17 references of the two plugins left out one way,
  7 masked Knights edits and 6 unseen Start Game Enabled quests the other).
  A save made in a cell or worldspace the session's plugins lack loads too
  (revision 31). Oblivion documents where: `SSaveGameSafeCellID:General`,
  default `2AEEA`, the Tiber Septim Hotel lobby, where "any actors whose cell
  has disappeared will be placed"
  ([UESP: Oblivion.ini](https://en.uesp.net/wiki/Oblivion_Mod:Oblivion.ini),
  [UESP: The Tiber Septim Hotel](https://en.uesp.net/wiki/Oblivion:The_Tiber_Septim_Hotel)),
  and the player is placed by that cell's spawn point. Fallout 3 and New
  Vegas ship the key, but the GECK wiki lists it as unused and `2AEEA` is no
  cell in either master; Skyrim, Fallout 4 and Starfield document nothing.
  There, by the owner's rule (2026-11-23), the load reloads the session's own
  location and the player keeps their pose in it. The report names the lost
  location and where the player is; both gates measure it (a save in
  LMCGone.esp's own room on FNV, one in Knights.esp's priory on Oblivion).
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
  First slice built (#6671): `byroredux … --preflight [--preflight-json
  <file>]` prints the plugin report (activation, roots, record conflicts,
  reverts, merged lists), then classifies every file each mod root ships by
  the catalog's `paths` / `game_dir_paths` / `ini_keys` patterns with its
  disposition (plugins with their activation state, archives with whether a
  plugin pairs with them, DLLs, plugin-named INIs key by key), scans every
  Papyrus script (loose and in paired archives) and every ObScript SCPT
  record of the root's plugins for extender calls (LMC-SCR-005), lists the
  catalogued files beside the game's executable and every asset path two
  sources ship with its winner, and sums coverage per disposition. A file,
  key or call no pattern covers is *unclassified* and fails §7.3 rule 2,
  which the conformance runner now measures. Not built yet: the unmanaged
  `Data` root's loose files (no official file list, LMC-REC-008), the probe
  answers (LMC-SCR-010, decision Q3), redundancy against the built-in fix
  catalog, and the launcher screen.
- **LMC-DX-002.** At runtime the debug CLI exposes the `mod.*` family
  ([DX-008](../engine/sandboxed-linked-mods.md#17-diagnostics-and-developer-experience)),
  `sdk.compat` (exists) and the fix report
  ([FIX-009](../engine/builtin-fixes.md#1-requirements)).
- **LMC-DX-003.** A conformance runner drives the corpus headlessly per game,
  opt-in like the game-data corpus tests, with the mod files supplied
  locally and never checked in.
  Built ahead of Phase 7 (#6676), so every later slice moves a measured
  number: `scripts/lmc-conformance.py` runs each installed corpus mod of
  `assets/legacy_mod_compat/corpus.toml` from a local corpus directory.
  Rule 1 of §7.3 is measured as activation (the profile, with masters
  resolved from the Data directory or other corpus mods, loads every plugin
  of the mod and disables none); rule 3 runs the mod's `[mod.scenario]`
  steps through `--bench-hold` and byro-dbg. Rule 2 is the same headless
  run's `--preflight`: every item the mod's root ships must be classified by
  the catalog. Rule 4 saves on the profile (where the scenario plays, else
  the fixture's cell), loads that save in a fresh process on the same
  profile and saves again, then loads it in a process without the mod's own
  plugins and root and saves again; every load must complete. A mod whose
  masters are not installed is blocked and left out of the rates.
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
  Measured (#6670): `cell_loader/load_order_scale_tests.rs` activates
  synthetic profiles of light plugins (the kind a 1,000-plugin profile can
  hold until LMC-ID-004), each with 100 own and 20 overriding records and a
  paired archive of 200 entries, through master hoisting and archive
  pairing, the fold, and the texture, mesh, sound and script pools. On the
  owner's machine (Ryzen 9 7950X, 16 cores; WD SN850X NVMe, ext4), 1,000
  plugins activate in 0.35 s with the page cache warm and in 0.33 s with the
  profile evicted from it first (`dd iflag=nocache`, confirmed by
  `fincore`): about 0.3 to 0.4 ms per plugin from 500 to 2,000. Archive
  pairing was quadratic (each plugin tested every archive name in its
  directory; 0.345 s at 2,000 plugins) and finds a plugin's archives by its
  stem since revision 38 (0.043 s). Opening archives is linear since
  revision 36. Not measured: plugins and archives of real mod sizes, which
  wait on the Wave 1 corpus, and regular plugins past 252, which wait on
  LMC-ID-004.
- **LMC-PERF-002.** Cost of the merged index is linear in records. A cell
  transition MUST NOT re-parse the load order on the main thread (#6591,
  fixed: one parse serves interior and exterior sessions, and a door
  transition reuses the resident index while the plugin set is unchanged).
  Measured (#6670): `cell_loader/load_order_scale_tests.rs` folds up to 4,000
  synthetic light plugins and fails if time per record triples. Per record
  the fold costs 0.6 to 0.9 µs from 250 to 4,000 plugins (31,000 to 481,000
  records), in a wide profile and in a chain where every plugin masters the
  one before it, and the same from 16,000 to 961,000 records at 500 plugins.
  The chain had a quadratic step, the transitive master closure built as a
  hash set per plugin (0.63 s at 4,000 plugins, now 0.43 s); it is a bitset
  row per plugin, and each plugin's masters are found through one name map
  built per fold, not by scanning the load order. The rise that remains,
  about 1.4 times over a 16-fold growth, appears with records as well as
  plugins and is not profiled.
- **LMC-PERF-003.** Asset lookup is an index hit, independent of archive
  count.
  Built (#6670): each provider pool is an `ArchiveSet` holding an index from
  the hash of each normalized path to the last-listed archive that holds it
  ([archives.md](../engine/archives.md#asset-provider-precedence-and-the-loose-tier)).
  An asset only the first of N synthetic archives holds cost 1.6 µs at 10
  archives and 64 µs at 400; it now costs 0.80 to 0.87 µs at any count. The
  index agrees with the archive scan on every entry of the real FNV, Skyrim
  SE and Fallout 4 archives tested, and a hit is checked against its archive
  so a hash collision costs a scan, never a wrong file. The script provider's
  short first-wins list is unchanged.
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

The Wave 1 candidates (Fallout: New Vegas and Skyrim SE) live with their
download links, dependencies, install tranche and verification status in
[`assets/legacy_mod_compat/corpus.toml`](../../assets/legacy_mod_compat/corpus.toml),
which `scripts/check-lmc-catalog.py` checks against the §3.1 catalog (every
mechanism a mod names must exist for its game). That file is the list the
conformance runner measures; the table below is the starting inventory it
was drawn from.

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
| 6 | 2026-11-04 | Phase 0 started (#6669): §3.1 is generated from `assets/legacy_mod_compat/mechanisms.toml` (65 rows, composite rows split, one disposition per game, versioned, cited; `scripts/check-lmc-catalog.py`, CI); the hand-written "Today" column is removed. Verified: the FO3 / FNV `<stem> - <anything>.bsa` pairing form (GECK) and Skyrim's `<stem>.bsa` (CK wiki). Still open, no primary source reachable: the rest of the Phase 0 verify list, corpus verification and scenarios, the honoured-INI table | — |
| 7 | 2026-11-22 | LMC-AST-002: owner ruling on #6670 — `.esm` and `.esl` plugins pair with archives as `.esp` plugins do. The FO4 voice suffix and Starfield's automatic pairing (with `- Localization` and `- Voices_<lang>`) verified from the shipped DLC; Skyrim's `- Textures.bsa` stays unverified and unpaired; the FO4 high-resolution pack's series is a new verify item. Pairing built in `crates/plugin/src/archive_pairing.rs` | — |
| 8 | 2026-11-22 | LMC-REC-003 built for the levelled lists (`merge` by default per Q2, `--merge-policy legacy`); the catalog's R2 and T2.necessity rows say so | — |
| 9 | 2026-11-22 | LMC-REC-003 extended to form lists, outfits and containers (form-list null slots keep their positions); the catalog's R2 and T2.necessity rows at version 3 | — |
| 10 | 2026-11-22 | LMC-REC-003 extended to actors (inventory, factions, spells, perks, keywords, actor values), races (spells, keywords, race relations) and carried items (keywords); AI packages and consumable effects stay one field, their order being their meaning; the catalog's R2 and T2.necessity rows at version 4 | — |
| 11 | 2026-11-22 | LMC-REC-003 extended to the keyword lists of activator, flora, furniture and container bases; an override that drops every keyword now clears them under both policies; locations and quests recorded as waiting on partial-form support; the catalog's R2 and T2.necessity rows at version 5 | — |
| 12 | 2026-11-22 | LMC-ID-012 built for the session profile (`--mod-root`, ownership, bare plugin names resolve through the roots); LMC-AST-010's root-ordered loose tier and LMC-ID-011's root half built with it; the catalog's A1, T10.vfs and T10.outputs rows at version 2 | — |
| 13 | 2026-11-22 | LMC-ID-007 and LMC-SAV-001 built: the profile record (plugins with content hashes, mod roots, extensions, grants, policy) and its fingerprint, saved as the `SessionProfile` column (save format v142); a load and `save.info` report every difference | — |
| 14 | 2026-11-22 | Phase 0 (§7.2): the Wave 1 corpus is checked in as `assets/legacy_mod_compat/corpus.toml`, 56 candidates (21 FNV, 35 Skyrim SE) with links, dependencies and install tranches; the catalog checker gains C10–C13 (corpus fields, mechanisms defined for the game, a verified mod carries a scenario and a licence). Additions to the §7.2 inventory: The Inheritance (content-only voiced quest), Extensible Follower Framework's SEQ file (R11), Player Character Gestures (OAR). No entry is verified yet | — |
| 15 | 2026-11-22 | LMC-DX-003 built ahead of Phase 7: the conformance runner (`scripts/lmc-conformance.py`) measures §7.3 rule 1 as activation and runs rule 3 scenarios, reporting rules 2 and 4 as not measurable until the preflight and profile-change saves exist; the corpus gains the `[mod.scenario]` shape and the catalog checker C14 for it. Moving the runner forward is the owner's call (2026-11-22); Phase 7's exit is unchanged | — |
| 16 | 2026-11-22 | LMC-ID-006 built ahead of Phase 2 at the owner's request: `--import-load-order` imports plugins.txt / loadorder.txt, MO2 profiles and Vortex deployments into the boot request's `[mods]` (plugins in load order, mod roots in priority order), with the per-game rules cited from libloadorder, MO2 and a live Vortex manifest; activation takes the unmanaged root from the base master `--game` appends, so an imported profile names its plugins by file name | — |
| 17 | 2026-11-22 | LMC-REC-006's merge-ownership half built: under `merge` an override identical to the record it would replace is a no-op without ownership, and an independent override identical to its master changes nothing; both are reported | — |
| 18 | 2026-11-23 | LMC-REC-007 recorded as built (fold provenance, `plugins.conflicts`, `--plugin-report`); the report's winner is the carrier that owns the record, so a no-op override is marked as one, and a byte difference in a field the merge does not read is labelled; measured on FNV with its nine DLC: 781 overridden forms, 11 of them kept by an earlier carrier | — |
| 19 | 2026-11-23 | LMC-REC-004 built for the cell index: cells, modelled base objects, pack-ins and the other form-keyed cell-index maps prune deleted forms (their prune was a no-op); measured on FNV, Skyrim SE and Fallout 4 + UFO4P against a raw record walk; a live reference to a deleted base and a deleted cell's uncovered references recorded as Phase 0: verify | — |
| 20 | 2026-11-23 | LMC-DX-001 first slice built (#6671): `--preflight` classifies every file a mod root ships by catalog patterns (catalog_version 2 adds `paths`, `game_dir_paths`, `ini_keys` and `[not_game_data]`, checked by C15), scans its Papyrus and ObScript scripts eagerly (LMC-SCR-005), names DLLs (LMC-EXT-001) and plugin-named INI keys (LMC-SET-001), lists asset conflicts and coverage; the conformance runner measures §7.3 rule 2 from it. LMC-REC-011 measured by the fold conformance gate | — |
| 21 | 2026-11-23 | LMC-SET-002 measured: 240 authored GMSTs read through the merged index, 8 named without a read; `fSkillUseCurve` and `fCombatDistance` were hard-coded and are now read (melee reach 141 on Skyrim, Fallout 4 and Fallout 76, 128 on Fallout 3 and New Vegas, where the engine used 180); `combat.approach` scales its ring with the reach; Oblivion's default and Starfield's meaning of `fCombatDistance` recorded as Phase 0: verify | — |
| 22 | 2026-11-23 | LMC-AST-005 measured on FNV, Skyrim SE and a Fallout 4 + SS2 profile: `Data\` / leading-separator stripping shared by meshes, textures and materials; `sound`, `scripts` and `lodsettings` loose roots and a loose script tier; Scaleform interface layering filed (#7336). The preflight stops counting base-game Papyrus calls as extender calls | — |
| 23 | 2026-11-23 | #7336: a mod's Scaleform movies and fonts (U1, U5) and MenuXml menus, prefabs, fonts and HUD textures (U3) resolve over the vanilla archives through every mod tier (`LayeredSources`), verified on the installed Skyrim SE and New Vegas archives; U4 set to D5 with a target of D1 until a source says who merges mods' translation files | — |
| 24 | 2026-11-23 | Catalog U4 sourced and built: SKSE64 and F4SE (`Translation.cpp`) merge every enabled plugin's `Interface\\Translations\\<stem>_<language>.txt`, UCS-2 LE, first definition kept; the engine does the same natively (U4 D1, Starfield D5 with no source). LMC-SAV-004's premise measured false for saved state (session-global FormIDs), which Phase 7 has to resolve first | — |
| 25 | 2026-11-23 | Phase 7 slice 1 (owner's option A): saved FormIDs as stable identities through `byroredux_core::stable_form_id`, the actor columns converted, save format v144 | — |
| 26 | 2026-11-23 | Phase 7 slice 2: the remaining post-reload saved FormIDs as stable identities (save format v146), with a guard classifying every save column | — |
| 27 | 2026-11-23 | Phase 7 slice 3: the pre-reload ledgers' FormIDs stable (v147); the live load reloads the session's plugins, drops a removed plugin's references and inventory stacks, and reports LMC-SAV-002/003/004's consequences | — |
| 28 | 2026-11-23 | LMC-REC-010 built: the sub-records no parser decodes are kept as opaque bytes for every plugin after the first, with the carrying plugin as provenance, and named by `plugins.conflicts`; "decoded" is measured per game and record type by removing each sub-record from sampled real records and re-parsing (the table is checked in and re-derived by ignored real-data tests); the official/user boundary is approximated by the first plugin until LMC-REC-008 | — |
| 29 | 2026-11-23 | Phase 7 measured on real data: the FNV profile-change save gate, and the Oblivion plugin-set gate updated to the session-profile load; the conformance runner measures §7.3 rule 4 (save, reload, the removal case) and LMC-DX-003's text says rule 2 is measured | — |
| 30 | 2026-11-23 | LMC-PERF-001 and -002 measured: the fold costs 0.6 to 0.9 µs per record from 250 to 4,000 light plugins and from 16,000 to 961,000 records; the chain profile's quadratic master closure became bitsets and master lookups share one name map; #6591 recorded as fixed | — |
| 31 | 2026-11-23 | LMC-SAV-002: a save made in a cell or worldspace the session's plugins lack loads, into Oblivion's documented safe cell (`SSaveGameSafeCellID`, default the Tiber Septim Hotel lobby) or, where no game documents one, the session's own location with the player's pose (owner's rule) | — |
| 32 | 2026-11-23 | LMC-UI-002 designed (`docs/engine/mcm-host.md`, owner's decisions: engine `SKI_ConfigBase`, native page, pause-menu entry); slice 1 built: a quest's own scripts receive `OnInit` once per save | — |
| 33 | 2026-11-23 | LMC-UI-002 slice 2: the engine `SKI_ConfigBase` and `SKI_PlayerLoadGameAlias` shadow SkyUI's, the `McmHost` model and static class, SkyUI's state-option dispatch, re-registration after every load, `mcm.*` console commands; `ShowMessage` declined | — |
| 34 | 2026-11-23 | LMC-UI-002 slice 3: the native Mod Configuration page (grid, five dialogs, unmap, blocking message) reached from the pause menu, the `McmHost` adapter with `$`-key translation, leaving the menus closes the page; a `.dds` custom panel is carried but not drawn | — |
| 35 | 2026-11-23 | A box wrapper (one `Return` body) called for its value hands its box to the calling statement, which waits and replays with the answer: SkyUI's `ShowMessage` raises a box with Accept / Cancel and returns the answer. A non-Run-Once Papyrus quest resets its own and its aliases' script state as it starts, then runs `OnInit`, before its startup stage (CK *OnInit*). Measured first: on vanilla Skyrim SE the general value-call wait declines 13 of 19,412 scripted references and 5 of 2,529 alias instances, so only the wrapper shape was built | — |
| 36 | 2026-11-23 | LMC-PERF-003 built: provider pools are `ArchiveSet`s with one path index each (a lookup costs 0.8 µs from 10 to 400 archives, was 1.6 to 64 µs), verified against the archive scan on real FNV, Skyrim SE and Fallout 4 archives; opening a pool's shadow counts became linear (400 archives: 21 s to 0.53 s), noted under LMC-PERF-001 | — |
| 37 | 2026-11-23 | LMC-UI-002 slice 4: MCM Helper (v1.6.2, version code 15) served by an engine `MCM_ConfigBase` and `MCM`; pages from `config.json` with mod-setting, script-property and global value sources, group conditions and actions; settings layered over the mod's defaults and an earlier install's file, written per game and named profile (`--profile-name`, a boot request's `[mods] name`; owner's decisions, mcm-host.md §6.4–6.6); hotkeys (`keybinds.json`) are slice 4b | — |
| 38 | 2026-11-23 | LMC-PERF-001 measured on synthetic profiles of light plugins with paired archives: 1,000 plugins activate in 0.35 s warm and 0.33 s cold on a Ryzen 9 7950X with NVMe; archive pairing made linear (`ArchiveDirectory`, 0.345 s to 0.043 s at 2,000 plugins); real mod sizes and regular plugins past 252 not measured | — |
| 39 | 2026-11-23 | LMC-UI-002 slice 4b: MCM Helper's `keybinds.json` hotkeys; a key map with an id binds a key (bindings per game and named profile, over an earlier install's); a press runs the function call, `SendEvent` delivers `OnControlDown` / `OnControlUp`; `RunConsoleCommand` is declined (no game-console interpreter) | — |
| 40 | 2026-11-23 | LMC-AST-009 measured: no `.xwm` decodes (Fallout 4's xWMA effects, 77% of its sounds, and Skyrim's and Fallout 4's music); catalog A7 split, A7.xwma at D5 with target D1, tracked by the xWMA decoder PRD | — |
| 41 | 2026-11-23 | LMC-UI-002 slice 5 (Fallout 4): FO4 MCM (1.40, version code 9) from its author documentation; configs listed by their `MCM/Config/<mod>` folders under the mod roots when their plugin or requirements are loaded; FO4's control names (button added; image, positioner and numeric text inputs left out, failed groups hidden); F4SE `RegisterForExternalEvent` served (per session) with `OnMCMSettingChange` / `OnMCMMenuOpen` / `OnMCMMenuClose`; `MCM.RefreshMenu()`; New Vegas' MCM is its own slice | — |
| 42 | 2026-11-23 | LMC-ID-010 built: a duplicate plugin entry is compared byte for byte with the earlier one and reported as a repeat or as an identity collision (the earlier loads); a mod root listed twice is one root, and two directories with one identity are an identity collision | — |
| 43 | 2026-11-23 | LMC-REC-009 built: every plugin is read as the profile's game (`ProfileGame` from the base master: record layouts and the TES4 light / medium bits, as xEdit's `IsLight`); LMC-ID-008 verified (the flags decide only the slot) with a fold warning for light / medium records past their object space | — |
| 44 | 2026-11-23 | LMC-SCR-004 built: `docs/data/extenders/*.tsv` (generated by `scripts/extender-catalog.py` from SKSE64's and F4SE's published scripts and changelogs, xOBSE's changelog and the GECK / CS wiki dumps; 3 709 functions, 322 with a documented introducing version), coverage measured by the VM checker and the SDK catalog and pinned (SKSE64 26 of 804 served, F4SE 4 of 227, xNVSE 4 of 565, OBSE 5 of 644); the scan (LMC-SCR-005) and the probe answers (LMC-SCR-010) are the next slices ([design](../engine/extender-catalog.md)) | — |
| 45 | 2026-11-23 | LMC-SCR-005 against the catalog: PEX call sites carry their receiver's declared type, resolved through the mod's scripts and the game's vanilla hierarchy (`docs/data/extenders/<family>-scripts.tsv`) to catalog rows; ObScript sources matched by command name; `--preflight` reports per family the functions called, served and not with their introducing versions, and counts receivers no table names | — |
| 46 | 2026-11-24 | Q5 built: PapyrusUtil `JsonUtil` and `StorageUtil.File*` in the Papyrus VM, in PapyrusUtil's file format (its `.psc` and file-layer source), writes confined to the profile's `moddata` directory over a mod's shipped files, bounded size, written when the game saves ([design](../engine/json-util.md)); path functions and `*ListRandom` not yet. LMC-ID-005 in the VM: SKSE `Game.GetModCount` / `GetModByName` / `GetModName` / `IsPluginInstalled` / `GetModDependencyCount` and the light-plugin family, over the SDK's adapters. The per-plugin sentinel beyond 254 regular plugins waits on LMC-ID-004 | — |
| 47 | 2026-11-24 | LMC-SCR-010 built: per mod and family, the highest level at which every called function from that level down is served, "absent" when an unserved call sits at the first release (SKSE64 2.0.0, F4SE 0.1.0); `GetVersionRelease` from release tables read from the version headers' history (`docs/data/extenders/<family>-releases.tsv`, documented from SKSE64 2.0.19 and F4SE 0.6.21; earlier levels answer 0); `--preflight` names each family's level and the calls that capped it; the VM serves `SKSE` / `F4SE` whole, per the calling script's source (its owned scripts, LMC-ID-012), `GetScriptVersionRelease` equal to the release and `GetPluginVersion` -1; ObScript probes wait on the NVSE byte forms ([design](../engine/extender-catalog.md) §7) | — |
| 48 | 2026-11-24 | Q5 completed: PapyrusUtil `StorageUtil`'s form-keyed values (saved, keyed by plugin-and-id text), its plucks / shifts / pops / sorts / prefix work / `debug_` listings, and `JsonUtil`'s path functions and `ListRandom` ([design](../engine/json-util.md)); JContainers 4.3.4 on a saved engine heap — owners and the documented 10-second temporary ownership by mark and sweep, `JDB` / `JFormDB`, JSON in JContainers' format, paths, `JAtomic` ([design](../engine/jcontainers.md)); Lua, collection operators, form-type filters not served. Format v150 | — |
| 49 | 2026-11-24 | LMC-SCR-004 catalog growth: the pure natives every SKSE mod calls run in the Papyrus VM, with the extenders' own behaviour from their native implementations at the pinned commits (byte strings as single-byte characters, C-locale classes, `UInt32` indices, case-insensitive `Find` / `Split`, arrays bounded as `new` is): SKSE64 `StringUtil` (11), `Math` bit operations and `Log` (7), `Utility` array creation and resizing (12); F4SE `Math` (8). Served: SKSE64 61 of 804, F4SE 17 of 227 ([design](../engine/extender-catalog.md) §8) | — |
| 50 | 2026-11-24 | LMC-SCR-004 catalog growth: SKSE's `Form.GetType` / `GetName` / `GetWeight` / `GetNumKeywords` / `GetNthKeyword` / `GetKeywords` / `HasWorldModel` / `GetWorldModelPath` and `HeadPart.GetType` on Skyrim, from `FormFacts` (every base record's signature and the values the index keeps) and SKSE64's form classes (`docs/data/extenders/skse64-form-types.tsv`, generated from its headers: `kFormType` numbers and the components each class carries); a component the class has but the engine does not read stops the call rather than answer empty; `IsPlayable` a declared gap (implemented in the game executable). The catalog checks each Papyrus family on its own game. Served: SKSE64 71 of 804 ([design](../engine/extender-catalog.md) §9) | — |
