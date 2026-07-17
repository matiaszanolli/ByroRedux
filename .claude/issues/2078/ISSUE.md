# FNV-D1-02: NifImportRegistry cache key has no plugin/archive-set discriminant — stale cross-load reuse via debug cell.load

- **Severity**: MEDIUM
- **Labels**: medium, import-pipeline, bug
- **Location**: `byroredux/src/cell_loader/nif_import_registry.rs:136-180`, `byroredux/src/cell_loader/references/mod.rs:610-617`, `byroredux/src/boot.rs:359`, `byroredux/src/debug_load.rs:206-266,268-360`

## Description
`NifImportRegistry` is a process-lifetime cache keyed only by lowercased model path (`references/mod.rs:608`: `let cache_key = model_path.to_ascii_lowercase();`) — no field records which BSA/BA2/ESM/load-order set resolved that path. `NifImportRegistry` has no `clear()` method at all, and it's inserted once at boot (`boot.rs:359`). Safe under the normal CLI launch (archive set is fixed for the whole run), but broken under the debug `cell.load` console command, which can synthesize an arbitrary `--bsa`/`--esm`/`--master` set per request against the *same* singleton registry — `exec_load_interior`/`exec_load_exterior` in `debug_load.rs` call `unload_current_interior`/`drain_streaming_state` but never touch the NIF import registry. A model-path collision across two such requests (e.g. comparing vanilla FNV against a mod's overriding BSA in one debug session) silently serves the first-loaded content for the second. The same pattern exists in `TextureRegistry::path_map`.

## Evidence
`references/mod.rs:608` keys the cache purely by lowercased path; no `clear()`/invalidation method exists on `NifImportRegistry`; `debug_load.rs`'s load-interior/load-exterior exec paths never call into the NIF import registry or texture registry to invalidate on an archive-set change.

## Impact
Wrong-but-plausible geometry/collision/texture silently substituted during exactly the inspect/compare workflow the debug tool exists for; easy to misattribute to a content/parser bug. Not reachable via the normal single-launch CLI path, so it doesn't affect bench-of-record numbers.

## Suggested Fix
Fold an archive-set identity/generation counter into the cache key, or have `exec_load_interior`/`exec_load_exterior` call `NifImportRegistry::clear()` (+ `TextureRegistry` equivalent) whenever the requested archive set differs from the previously-loaded one.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (`TextureRegistry::path_map` has the identical no-discriminant cache key)
- [ ] **TESTS**: A regression test pins this specific fix (e.g. two `cell.load` invocations with different `--bsa` sets against the same model path resolve independently)
