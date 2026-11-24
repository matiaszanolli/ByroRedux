# PapyrusUtil Host (JsonUtil and StorageUtil) — Design

**Status**: BUILT (2026-11-24), for the [Legacy Mod Compatibility PRD](../prd/legacy-mod-compatibility.md)
decision Q5 ("supported, confined to a per-mod data directory with bounded size") and LMC-SCR-004. Code:
`crates/scripting/src/json_util.rs` (the documents, the file store and `StorageUtil`'s saved values),
`crates/scripting/src/fragment/vm/natives/json_util.rs` (the Papyrus natives), `byroredux/src/mcm_files.rs` (the
files, shared with [JContainers](jcontainers.md)).

## 1. What mods expect

PapyrusUtil's `JsonUtil` lets a script keep values in JSON files that outlive a save: configuration,
presets, and data mods share. The contract comes from PapyrusUtil's own declarations,
`Scripts/Source/JsonUtil.psc` and `StorageUtil.psc` (`eeveelo/PapyrusUtil@ff854180db67`):

- A file is named relative to `Data/SKSE/Plugins/StorageUtilData/`. `.json` is appended unless the name holds
  it, and a name may climb: `"../MyData/config"` is `Data/SKSE/Plugins/MyData/config.json`.
- "You do not need to call Load() or Save() manually": a file is read on first use, and "when the player saves
  their game any modified file will be automatically saved". Loading another save without saving discards the
  changes.
- The functions "work in exactly the same way as their StorageUtil.psc equivalents": values are separate per
  type (int, float, string, form, and a list of each), and key names are not case sensitive.
- `StorageUtil`'s `File*` functions are `JsonUtil` on `"../StorageUtil.json"` (their bodies in
  `StorageUtil.psc`).

The on-disk format is the one PapyrusUtil writes, so a file a mod ships, or an earlier PapyrusUtil install
wrote, reads unchanged (PapyrusUtil source: `External.h`, `External.cpp`, `Forms.cpp`). It is one object with
the sections `int`, `float`, `string`, `form`, `intList`, `floatList`, `stringList` and `formList`, each mapping
a lowercased key to its value or list. A form is `"0x<local id, hex>|<plugin file>"`, and `"0"` is None.

## 2. The engine's host

- **Store.** `JsonUtilStore` (a resource) keeps the open files in memory. It writes each changed one when the
  game saves (`save_now`, after the save commits) and drops them all on a load (`live_load`). The files are
  the state: the store is not saved.
- **Where files live** (Q5, SEC-003). A name resolves inside `SKSE/Plugins/` and never above it: a `..` past
  it, a drive or a control character is refused, and the call answers its "missing" value. The engine writes
  only to the profile's own directory, `<settings.toml's directory>/moddata/<base master>/<profile>/` by the file's
  path under `Data/` (so `SKSE/Plugins/StorageUtilData/…` there),
  the same per-game, per-profile scheme MCM Helper's settings use. It reads that copy first, else a mod root's
  shipped `SKSE/Plugins/…` file (loose files, as PapyrusUtil reads them).
- **Bounds** (engine rules, the "bounded size" of Q5). A file is at most 4 MiB, read or written, and a session
  keeps at most 256 files open. `*ListResize` stops at 500 entries (`StorageUtil.psc`).
- **Forms** go through the session's load order: a form is written with its plugin's file name and local id,
  and read back through the same identity, so a reorder does not change what a file names. A temporary form
  (`…|<type>.FF`), and a form not in the load order, read as None.

## 3. StorageUtil's values on forms

`StorageUtil`'s own functions keep values on a form or globally (`ObjKey` None), and they ride the save.
`StorageUtil.psc` says: "Values will stay on forms or globally until they are Unset or Cleared".
`StorageUtilValues` (saved) holds one document per object, keyed by the form's `0x<local id>|<plugin file>` text,
`""` globally. A save therefore names its forms by plugin and id, and a changed load order still finds them. A form
outside the load order is keyed by its session id (`0x<id>|FF`) and means nothing after a load, like PapyrusUtil's
temporary forms. "If zero, empty, or none are given, the key will be unset."

## 4. Served and not served

Served on both, a `JsonUtil` file or a `StorageUtil` object:
- for each type, `Set`, `Get`, `Unset` and `Has`, plus `Adjust` for int and float;
- for each list type, `Add`, `Get`, `Set`, `Remove`, `InsertAt` (`StorageUtil`'s `Insert`), `RemoveAt`, `Clear`,
  `Count`, `CountValue`, `Find`, `Has`, `ToArray`, `Resize`, `Slice`, `Copy` and `Random`, plus `Adjust` for int and
  float;
- the `Count*Prefix` family.

`JsonUtil` also serves:
- `Load`, `Save`, `Unload`, `IsPendingSave`, `IsGood`, `GetErrors`, `JsonInFolder`, `JsonExists` and `ClearAll`;
- the path functions (`SetPath*`, `GetPath*`, `Path*Elements`, `FindPath*Element`, `PathCount`, `PathMembers`,
  `CanResolvePath`, `IsPath*`, `SetPath*Array`, `ClearPath`, `ClearPathIndex`, `SetRawPathValue`) over jsoncpp
  paths: `.member` and `[index]`, created as needed on a set.

`StorageUtil` also serves:
- `Pluck`, and the list `Pluck`, `Shift`, `Pop` and `Sort`;
- the prefix counts and clears across every object or on one (`Count…Prefix`, `CountObj…Prefix`, `Clear…Prefix`,
  `ClearObj…Prefix`, `…AllPrefix`);
- the `debug_` listings of objects and keys, and `debug_DeleteValues` / `debug_DeleteAllValues`;
- its `File*` functions, which are `JsonUtil` on `../StorageUtil.json`.

Not served, each a VM stop that names it:
- `FormListFilterByType(s)`, which needs Papyrus form-type ids the engine does not map;
- `ImportFile` / `ExportFile`;
- `debug_Cleanup` and `debug_SetDebugMode`.

A sandboxed SDK extension still cannot call `JsonUtil` (`classify_static_call`): it has no host files, and its
data goes through principal storage. Only a legacy mod's Papyrus script, run by the engine's VM, is served.

## 5. Deliberate choices

- **"Per-mod" is per profile.** PapyrusUtil names files globally, and mods read each other's files by name. So
  the confinement is one directory per profile and game, not one per mod. A script cannot name a file outside
  that tree, and the engine never writes to a mod's own folder.
- **`ListSet` returns the previous value**, as `StorageUtil.psc` documents ("This will return the previous value
  or 0 if there was a problem").
- **`Set*Value` stores the value given, zero included.** `StorageUtil.psc` says a zero or empty value unsets a
  form-keyed value, but PapyrusUtil's file layer (`External.cpp`, `SetValue`) stores it, and the file layer is
  what `JsonUtil` uses.
