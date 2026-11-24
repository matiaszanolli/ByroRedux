# JContainers Host — Design

**Status**: BUILT (2026-11-24), for the [Legacy Mod Compatibility PRD](../prd/legacy-mod-compatibility.md)
decision Q5 and LMC-SCR-004. Code: `crates/scripting/src/jcontainers.rs` (the heap),
`crates/scripting/src/fragment/vm/natives/jcontainers.rs` (the Papyrus classes). Files go through the mod data files
the [PapyrusUtil host](json-util.md) uses (`byroredux/src/mcm_files.rs`).

## 1. What mods expect

JContainers gives Papyrus scripts arrays and maps they can nest, keep across saves, and read from and write to
JSON. The contract is JContainers' own: its API declarations (`src/JContainers/src/api_3/tes_*.h`), its JSON layer
(`collections/json_serialization.h`), and its wiki pages *Lifetime Management*, *Serialization* and *Path
Resolving*. All are cited at `ryobg/JContainers@0dcf61e0aa04` (version 4.3.4.0, MIT).

- **Objects are integer handles.** `JArray`, `JMap` (string keys, compared without case), `JFormMap` (form keys)
  and `JIntMap` (integer keys) hold ints, floats, strings, forms, none, and other objects.
- **Lifetime.** "As long as an object has at least one owner, it continues to exist." The owners are a `retain`,
  a container holding the object, and `JDB` or `JFormDB` reaching it. A new object, or one returned to Papyrus, is
  owned temporarily "for around 10 seconds"; after that "an object which is not referenced directly or indirectly
  by `JDB` or `JFormDB` gets destroyed".
- **Saves.** "When a save is performed, all objects are saved and all objects are restored when the save file gets
  loaded."
- **JSON.** A `JArray` is `[]` and a `JMap` is `{}`. A `JFormMap` or `JIntMap` is `{}` marked
  `"__metaInfo": {"typeName": …}`, its keys form texts or decimal integers. A form is `"__formData|Plugin|0x…"`
  (`"__formData||0x…"` for a dynamic form), none is `null`, and an object met twice is written once and then linked
  as `"__reference|<path>"`.
- **Paths.** `.key` into a `JMap`, `[index]` into a `JArray` (a negative index counts from the end),
  `[__formData|Plugin|formId]` into a `JFormMap`, `[n]` into a `JIntMap`. With `createMissingKeys`, a setter
  "will insert any missing `JMap` key during path traversal".
- **`JFormDB`** keeps per-form entries as `JDB[storageName]`, a `JFormMap` from form to entry `JMap`. Its paths are
  `.storageName.rest`.

## 2. The engine's host

- **One heap per session** (`JcHeap`, saved). It holds the objects, their retain counts and tags, and `JDB`'s root.
  Forms are kept by plugin identity and local id (`FormRef`), so a changed load order still names them. A form
  outside the load order is kept by its session id.
- **Collection** marks from the owners and sweeps the rest. The owners are retains, `JDB`, and the temporary owners
  whose 10 seconds have not run out. The sweep runs as the heap grows (at twice the last live count, at least 4,096)
  and on demand.
  - Every handle a script passes or receives renews the object's temporary owner. An object in use is therefore
    never swept: the heap may keep an object longer than JContainers would, never shorter.
  - A load renews every restored object's temporary owner, so nothing a script is about to use goes before it runs.
  - Temporary owners count real seconds from the process start.
- **Bounds** (Q5): at most 262,144 live objects; a file is at most 4 MiB, read or written.
- **Files.** `readFromFile`, `writeToFile`, `readFromDirectory` and `JDB.writeToFile` name paths from the game
  folder. A leading `Data/` is dropped, and a path that leaves `Data/` is refused. Reads take the profile's copy,
  else a mod's shipped file; writes go to the profile's `moddata` directory, never a mod's folder.
  `JContainers.userDirectory()` is `Data/SKSE/Plugins/JCUser/` there.
- **Version.** `JContainers.isInstalled()` answers true, `APIVersion()` 4, `featureVersion()` 3, `versionString()`
  `4.3.4.0`. A mod's own `JContainers.pex`, comparing against the version it was built with, reads the same numbers.

## 3. Served and not served

Served:
- **`JValue`:**
  - lifetime: `retain`, `release`, `releaseAndRetain`, `releaseObjectsWithTag`, `zeroLifetime`, `addToPool`,
    `cleanPool`;
  - `shallowCopy` and `deepCopy`;
  - type tests, `empty`, `count`, `clear`;
  - files: `readFromFile`, `readFromDirectory`, `objectFromPrototype`, `writeToFile`, `toJsonString`;
  - paths: `solvedValueType`, `hasPath`, `solve*` and `solve*Setter`;
  - `enableAPILog`, which does nothing.
- **`JArray`:**
  - constructors: `object`, `objectWithSize`, `objectWith*`, `subArray`, `addFromArray`;
  - typed access: `get*`, `set*`, `add*`, `as*Array`;
  - searching: `find*`, `count*`;
  - erasing: `erase*`, `eraseIndex`, `eraseRange`;
  - the rest: `count`, `clear`, `valueType`, `swapItems`, `sort`, `unique`, `reverse`.
- **`JMap`, `JFormMap`, `JIntMap`:**
  - `object`;
  - typed access: `get*`, `set*`, `insert*`;
  - keys: `hasKey`, `valueType`, `removeKey`, `allKeys`, `allKeysPArray`, `allValues`, `nextKey`, `getNthKey`;
  - `count`, `clear`, `addPairs`.
- **`JDB`:** `solve*`, `solve*Setter`, `setObj`, `hasPath`, `allKeys`, `allValues`, `writeToFile`, `root`.
- **`JFormDB`:** `setEntry`, `makeEntry`, `findEntry`, `solve*`, `solve*Setter`, `hasPath`, `allKeys`, `allValues`,
  `get*`, `set*`.
- **`JAtomic`:**
  - `fetchAdd`, `fetchMult` and `fetchDiv` (int and float);
  - `fetchMod`, `fetchAnd`, `fetchXor` and `fetchOr` (int);
  - `exchange*` and `compareExchange*`.
- **`JString`:** `wrap`.
- **`JContainers`:** the version functions, `fileExistsAtPath`, `userDirectory`, `lastError` and `lastErrorString`.

Not served, each a VM stop that names it:
- `JLua` and `JValue.evalLua*`. Lua is outside the engine.
- Collection operators in paths (`@…`). The wiki's path page does not document them.
- `JArray.addFromFormList` and `JArray.writeTo*PArray`.
- `JContainers.removeFileAtPath` and `JContainers.contentsOfDirectoryAtPath`.

A sandboxed SDK extension keeps the SDK's own bounded container service (`byroredux_sdk::legacy_containers`). That
service is deterministic: an unowned object goes the moment its last owner lets go. It is not this heap, whose
lifetime follows JContainers' documented temporary ownership. The two share no handles.

## 4. Deliberate choices

- **A heap of its own, not the SDK's container registry.** The SDK registry is built for a sandboxed guest: 256
  objects, immediate collection, and an owner count that scans every entry on each release. Mods' scripts need
  JContainers' 10-second temporary ownership, form- and int-keyed maps, `JDB`, and many thousands of objects.
- **Temporary ownership is renewed on every use** (see §2), a superset of JContainers' rule.
- **`JMap` key order** is by lowercased key, and `JFormMap` key order is by plugin identity and local id. Neither is
  JContainers' load-order comparer, so `allKeys`, `nextKey` and `getNthKey` walk in that order.
- **String comparison** in `find*`, `count*` and `erase*` ignores case, as `FindPathElement` does in PapyrusUtil;
  JContainers' own comparer is not documented.
