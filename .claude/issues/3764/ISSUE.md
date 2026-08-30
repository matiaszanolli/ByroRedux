# #3764 — SAFE-2026-08-30-D8-01: embedded-clip registration bypasses the #790 path memo on the per-NPC spawn path, plus an unparented `AnimationPlayer` entity

**Labels**: bug, animation, medium, memory, safety

---

- **Severity**: MEDIUM
- **Dimension**: 8 — NPC / animation spawn safety
- **Location**: `byroredux/src/scene/nif_loader.rs` — the embedded-clip branch inside `load_nif_bytes_with_skeleton`; callers `byroredux/src/npc_spawn/resumable.rs` (skeleton / body / armor / head)
- **Source**: `docs/audits/AUDIT_SAFETY_2026-08-30.md` (`SAFE-D8-01`), HEAD `64f64480`

## Description

`AnimationClipRegistry::get_or_insert_by_path` is the **#790** dedup mechanism, and it is
correct and case-insensitive. But `load_nif_bytes_with_skeleton`'s embedded-clip branch
calls the **un-keyed `registry.add(clip)`** instead (verified verbatim at HEAD).

That function is invoked once per NPC skeleton, once per NPC body part, once per head, and
once per equipped item — so **any NPC-worn NIF that carries an embedded controller stack
registers a fresh full clip copy on every NPC spawn, and again on every cell reload**.

Nothing releases these handles: the two `release()` call sites
(`byroredux/src/streaming_helpers.rs`, `byroredux/src/cell_loader/references/mod.rs`) only
retire handles owned by the cell-loader's `NifImportRegistry` LRU. The `SceneImportCache`
consulted earlier in the same function memoises the *parse/import* (`ImportedScene`), **not
the registration** — a cache HIT still falls through to the `registry.add(clip)`.

## Evidence

- `byroredux/src/scene/nif_loader.rs` —
  `let mut registry = world.resource_mut::<AnimationClipRegistry>(); registry.add(clip)`.
  Compare `byroredux/src/npc_spawn.rs`, the sibling KF loader, which **correctly** uses
  `registry.get_or_insert_by_path(kf_path.to_string(), || clip)`.
- `crates/core/src/animation/registry.rs` — the `plain_add_does_not_populate_path_map` test
  documents that `add()` is deliberately outside the dedup map, so this is a **genuine
  opt-out, not a latent memo hit**.
- The cell-loader REFR path does **not** have this gap — it memoises the handle on
  `CachedNifImport` (`byroredux/src/cell_loader/nif_import_registry.rs`,
  `byroredux/src/cell_loader/partial.rs`) and releases it on eviction. **The NPC path is the
  one that skipped it.**
- The same call site also `world.spawn()`s an `AnimationPlayer` entity **with no `Parent`
  link to `placement_root`**, so it is outside the subtree a cell unload despawns — a
  second, entity-level leak on the same path.

## Impact

One un-freeable `AnimationClip` (keyframe arrays + text keys + channel HashMap) **plus one
orphan ECS entity** per NPC-part NIF carrying an embedded clip, per NPC, per cell load.
This is exactly the **#790** failure shape (steady RAM growth across a walking session)
reintroduced on a different caller.

**Magnitude is content-dependent and was not measurable in this run** — no game archives
are present in the checkout and the runtime baselines under
`.claude/audit-baselines/runtime/` carry no embedded-clip counter — so this is rated MEDIUM
rather than the HIGH the "leak that compounds per cell" anchor would give a *confirmed*
per-cell leak. **If a census shows FNV/Skyrim NPC body/armour NIFs commonly carry
`NiControllerSequence` / inline transform controllers, escalate to HIGH.**

## Suggested Fix

Route the embedded-clip registration through the same memo as the KF path —
`registry.get_or_insert_by_path(label.to_ascii_lowercase(), || clip)`, keyed on the `label`
parameter already threaded into `load_nif_bytes_with_skeleton` (it is the archive mesh path
at every `resumable.rs` call site). Separately, **parent the spawned `AnimationPlayer`
entity to the mesh root** so cell unload reclaims it. Add a registry-length assertion to a
two-NPC spawn test to pin it.

## Related

#790 (E-N1, the original grow-only leak, CLOSED), #866 (case folding, CLOSED),
#863 + #2524 (LRU release wiring, CLOSED), **#2689 (OPEN — slot-header stranding;
`release()` never returns a slot to a free list, so even a released clip strands one
header)**, #3377 (`PersistentCellApplyJob` leaking pending clip handles, CLOSED).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — every `registry.add(` call site outside the KF loader
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved (`StringPool` is dropped before `AnimationClipRegistry` is taken today — preserve that)
- [ ] **TESTS**: A regression test pins this specific fix — spawn two NPCs sharing one part NIF and assert the registry length is unchanged on the second
