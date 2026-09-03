# #3760 — SAFE-2026-08-30-D3-01: SceneImportCache is an uncapped, never-evicted process-lifetime hold

**Severity**: MEDIUM · **Location**: `byroredux/src/scene_import_cache.rs`
**Source**: `docs/audits/AUDIT_SAFETY_2026-08-30.md` (SAFE-D3-01)

`SceneImportCache` wraps `ParsedNifCache<ImportedScene>` for the hierarchical
NIF-import path (`load_nif_bytes_with_skeleton`). Its sibling wrapper
`NifImportRegistry` grew an LRU cap under #635 (default 2048 entries,
`BYRO_NIF_CACHE_MAX` override) because the shared core deliberately does no
eviction of its own. `SceneImportCache` never got the equivalent — no
`max_entries`, no eviction, no `remove`/`clear` call site anywhere. Each
entry is a full `Arc<ImportedScene>` (roughly 60+ bytes per vertex retained
on the CPU heap in addition to the GPU copy) — a substantially heavier
per-entry payload than any of the three caches already capped, populated
not just by NPC skeletons/bodies/heads but the armor/outfit spawn phase
too, whose distinct-mesh population across a long Skyrim SE/FO4 streaming
session is hundreds-to-thousands.

## Fix implemented

Per the issue's own "minimal" suggested-fix option: the half-eviction-on-
overflow pattern `MaterialProvider`'s `bgem_cache`/`failed_paths` already
establish (#951/#1430), rather than porting `NifImportRegistry`'s full
LRU-with-clip-handle-eviction-bias machinery —`SceneImportCache` has no
clip-handle bookkeeping of its own to protect (only `NifImportRegistry`
manages `AnimationClipRegistry` handles), so that machinery's extra
complexity doesn't apply here.

- Added `insertion_order: VecDeque<String>` (insertion-order key tracker),
  `max_entries: usize`, and `evictions: u64` to `SceneImportCache`.
- `new()` reads the *existing* `BYRO_NIF_CACHE_MAX` env var — reused, not a
  new knob, for the `=0` unlimited escape hatch the issue asked for parity
  on — but defaults to a new `DEFAULT_MAX_ENTRIES = 300` when unset, much
  lower than `NifImportRegistry`'s 2048 given this cache's heavier
  per-entry payload (the issue's own "a few hundred" framing).
  `BYRO_NIF_CACHE_MAX=0` still disables the cap and warns at startup,
  matching `NifImportRegistry`'s existing convention.
- `insert` evicts the oldest `max_entries / 2` distinct keys (by insertion
  order) once a *new* key would overflow the cap, before inserting —
  re-inserting an already-cached key (overwrite) does not double-count it
  in the insertion-order tracker or trigger a spurious eviction.

**SIBLING** (issue's own checklist item): grepped every `ParsedNifCache<`
consumer in the tree — only `SceneImportCache` and `NifImportRegistry`
exist; no third wrapper has been added since. Nothing else to cap.

**LOCK_ORDER** (issue's own checklist item): no RwLock scope changed —
`SceneImportCache` is a plain `Resource` with its own `&mut self` methods,
same as before.

**TESTS** (issue's own checklist item — "insert past the cap and assert the
entry count stops growing"): four new tests mirroring
`nif_import_registry_tests.rs`'s own conventions (a `cache_with_cap`
helper bypassing the env var, matching `registry_with_cap`) —
`half_eviction_removes_oldest_entries_on_overflow` (insert past a cap of
4, confirm the two oldest are evicted and the entry count stays bounded),
`small_session_under_cap_never_evicts` (also pins the default cap value),
`unlimited_mode_never_evicts` (`BYRO_NIF_CACHE_MAX=0` parity), and
`re_inserting_an_existing_key_does_not_inflate_the_eviction_queue` (the
overwrite-safety property described above).

Full workspace: `cargo test --no-fail-fast` 7073 passing, 0 failing (+4 new
tests).
