# JsonUtil Host — Design

**Status**: BUILT (2026-11-24), for the [Legacy Mod Compatibility PRD](../prd/legacy-mod-compatibility.md)
decision Q5 ("supported, confined to a per-mod data directory with bounded size") and LMC-SCR-004. Code:
`crates/scripting/src/json_util.rs` (the store), `crates/scripting/src/fragment/vm/natives/json_util.rs` (the
Papyrus natives), `byroredux/src/mcm_files.rs` (the files).

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
  only to the profile's own directory, `<settings.toml's directory>/moddata/<base master>/<profile>/skse/plugins/`,
  the same per-game, per-profile scheme MCM Helper's settings use. It reads that copy first, else a mod root's
  shipped `SKSE/Plugins/…` file (loose files, as PapyrusUtil reads them).
- **Bounds** (engine rules, the "bounded size" of Q5). A file is at most 4 MiB, read or written, and a session
  keeps at most 256 files open. `*ListResize` stops at 500 entries (`StorageUtil.psc`).
- **Forms** go through the session's load order: a form is written with its plugin's file name and local id,
  and read back through the same identity, so a reorder does not change what a file names. A temporary form
  (`…|<type>.FF`), and a form not in the load order, read as None.

## 3. Served and not served

Served: `Load`, `Save`, `Unload`, `IsPendingSave`, `IsGood`, `GetErrors`, `JsonInFolder`, `JsonExists`,
`ClearAll`; for each type `Set`, `Get`, `Unset`, `Has` (and `Adjust` for int and float); for each list type
`Add`, `Get`, `Set`, `Remove`, `InsertAt`, `RemoveAt`, `Clear`, `Count`, `CountValue`, `Find`, `Has`,
`ToArray`, `Resize`, `Slice`, `Copy` (and `Adjust` for int and float); the `Count*Prefix` family; and
`StorageUtil.File*` for each of these.

Not served yet, each a VM stop that names it: the experimental path functions (`SetPath*`, `GetPath*`,
`Path*Elements`, `FindPath*Element`, `PathCount`, `PathMembers`, `CanResolvePath`, `IsPath*`, `ClearPath*`,
`SetRawPathValue`), and `*ListRandom`. `StorageUtil`'s form-keyed values are save state, a different service,
and stay unserved.

A sandboxed SDK extension still cannot call `JsonUtil` (`classify_static_call`): it has no host files, and its
data goes through principal storage. Only a legacy mod's Papyrus script, run by the engine's VM, is served.

## 4. Deliberate choices

- **"Per-mod" is per profile.** PapyrusUtil names files globally, and mods read each other's files by name. So
  the confinement is one directory per profile and game, not one per mod. A script cannot name a file outside
  that tree, and the engine never writes to a mod's own folder.
- **`ListSet` returns the previous value**, as `StorageUtil.psc` documents ("This will return the previous value
  or 0 if there was a problem").
- **`Set*Value` stores the value given, zero included.** `StorageUtil.psc` says a zero or empty value unsets a
  form-keyed value, but PapyrusUtil's file layer (`External.cpp`, `SetValue`) stores it, and the file layer is
  what `JsonUtil` uses.
