# PERF-SD-2026-10-19-03: `step_created_references` runs every frame for the rest of the session once any created reference exists, scales with every record ever created, and `locate_refr` is a full-index scan

Labels: low,performance,scripting,bug
URL: https://github.com/matiaszanolli/ByroRedux/issues/6125

**Severity**: LOW · **Dimension**: CPU Hot Paths / Streaming & Cells

## Location
- `byroredux/src/app_step.rs` `step_created_references` (called from `step_cell_transition` each frame)
- `byroredux/src/cell_loader/created.rs` (`place_created_references`, `unspawned_in`)
- `crates/scripting/src/created_references.rs` (records are never pruned: `retain_quests` only touches `by_alias`; `scripted` and `Deleted` tombstones stay, and are saved)
- `crates/plugin/src/esm/cell/index.rs` `locate_refr`

## Description
The early-out is `created.iter().next().is_none()`. The first record ever created (quest alias or a script `PlaceAtMe`, which five different ObScript/Papyrus call sites feed) makes the step run every frame forever, because tombstones are kept "so the alias refresh does not create it again". Each frame `place_created_references` iterates all records; in an exterior session the step builds a `Vec` of the loaded cells, clones the worldspace `String` once per loaded cell, and calls `unspawned_in` per cell, which iterates all records again and (for a cell with a live record) builds a `HashSet`. In an interior session every spawn event (a wanted, not-live record) rebuilds both archive providers from the command line (`build_texture_provider` + `build_material_provider`) instead of reusing a kept provider as `step_actor_graphs` does with `ActorClipProvider`; archive handles are shared (`open_shared`, ~0.1 ms per #5288). Separately, a created record whose target cannot be placed ("its target has no placement yet") is retried every frame, and each retry calls `locate_refr`, a miss that walks every `PlacedRef` in the index.

## Evidence
Scratch replica of the exterior loop (49 loaded cells, records mostly `Deleted` tombstones, release): 10 records 0.8 us/frame, 100 -> 4 us, 1,000 -> 32 us, 10,000 -> 276 us. `locate_refr` miss scan (scratch replica, 512-byte stride, real `PlacedRef` size not measured): 63k REFRs 0.10 ms, 800k 5.7 ms, 1.55M 8.7 ms. `locate_refr` confirmed at publish time as a linear `iter().find` over every cell's references.

## Impact
Negligible for a normal session (tens of records, a few us per frame). It becomes a per-frame cost only with unbounded `PlaceAtMe` churn, and a stuck record on a large index costs a few ms per frame. Not a bottleneck today.

## Suggested Fix
Gate the step on a dirty key (record count / generation + `loaded_residency_changed` / interior root change); prune `Deleted` tombstones whose quest is gone; add a lazily built `form_id -> locator` map to `EsmCellIndex` next to the existing `cell_by_form_id` `OnceLock` pattern and use it in `locate_refr` (also used once per door by `queue_transition_to_reference` and `interior_spawn.rs`).

Source: docs/audits/AUDIT_PERFORMANCE_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
