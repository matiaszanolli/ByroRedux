# FNV-D1-02: NIF-cache clip-handle commit ordered after insert-driven LRU eviction (latent leak; unreachable on FNV)

**Source audit**: `docs/audits/AUDIT_FNV_2026-07-02.md` (finding FNV-D1-02)
**GitHub issue**: https://github.com/matiaszanolli/ByroRedux/issues/1854
**Labels**: low, import-pipeline, legacy-compat, bug

**Severity**: LOW
**Dimension**: Cell Loading (NIF import registry)
**Location**: `byroredux/src/cell_loader/references.rs:745-757`; `byroredux/src/cell_loader/nif_import_registry.rs:221-223`
**Status**: NEW

## Description

In the end-of-load batched commit, `pending_new` entries are `insert()`ed first (running LRU eviction), and `pending_clip_handles` are committed via `set_clip_handle` **afterward**. `set_clip_handle` (`nif_import_registry.rs:221`) inserts unconditionally with no check the key is still resident in `core.cache`. If a key inserted earlier in the same loop were evicted by a later insert in the same loop, its clip handle would be set for an already-evicted key: never `release()`d (keyframe-array leak) plus a dangling `clip_handles` entry lingering until `clear()`.

## Evidence

`references.rs:749` `freed.extend(reg.insert(key, entry));` then `references.rs:755-757` `for (key, handle) in pending_clip_handles { reg.set_clip_handle(key, handle); }` — no residency check. `set_clip_handle` body is a bare `self.clip_handles.insert(key, handle)`.

## Impact

Reachable only when a single `load_references` call inserts more than `BYRO_NIF_CACHE_MAX` (default 2048) unique NIFs so its own inserts evict each other. `load_references` runs per-cell (`exterior.rs:403`; the pre-M40 "all 49 cells in one call" path is gone), and no single FNV cell approaches 2048 unique models. **Not reachable on vanilla FNV**; latent hardening only.

## Suggested Fix

Commit `pending_clip_handles` before the insert loop, or guard `set_clip_handle` on `self.core.get(&key).is_some()` and release the handle otherwise.

## Completeness Checks
- [ ] **SIBLING**: Check other batched end-of-load commit sites for the same insert-then-commit-metadata ordering assumption
- [ ] **TESTS**: A regression test drives more than `BYRO_NIF_CACHE_MAX` unique keys through a single batched commit and asserts no clip handle is set for an evicted key
