# #1195 + #1196 Investigation (paired close)

## Blocker status

- **#1194 (PERF-DIM7-INSTR — instrumentation)**: CLOSED COMPLETED. `SkinCoverageFrame.dispatches_skipped` counter and GPU timer brackets already in place at `crates/renderer/src/vulkan/skin_compute.rs:113` and `context/draw.rs:907-938 / 1027-1076`.
- **#1196 (PERF-DIM7-02 — BLAS refit gate)**: OPEN, paired with this one. Audit explicitly says "MUST gate the BLAS refit on the same bool. Split decisions are the trap." Fixed atomically in commit `57c34c7f`.

## Strategy selection

Audit offered two:
1. **CPU hash gate** — hash per-entity bone_world slice, compare to last frame
2. **ECS-side gate** — route `AnimationPlayer::dirty` through SkinnedMesh query

Picked Strategy 1 for three reasons:
1. `AnimationPlayer` has no `dirty` field today; adding one + flipping it from the animation system is a bigger surface
2. Strategy 2 misses script-driven and physics-driven pose changes on entities without an `AnimationPlayer` (FNV / FO3 NPC kf-era entities, ragdolls)
3. Strategy 1 is also future-proof — if AnimationPlayer.dirty lands later, the hash gate is still correct (just becomes redundant for that specific path)

## Hash choice

Rolled a small inline FNV-1a over `f32::to_bits() as u64` because:
- No new crate dependencies (per CLAUDE.md "No new dependencies without user approval")
- No unsafe (safe `to_bits()` instead of raw byte cast)
- ~16 ops/matrix × 32 bones = 512 ops × 34 entities ≈ 17k ops/frame ≈ ~17 µs total — well below the ~5 µs/slot × 34 slot dispatch cost it avoids
- Deterministic across frames; endian-independent
- NaN handling: `f32::to_bits` is bitwise — different NaN encodings hash differently; that's correct fail-open behaviour (treats unusual NaN states as dirty)

`std::hash::DefaultHasher` (SipHash) would have been ~3-5× slower and gated the savings; ahash / xxhash3 would need a new dep.

## Regression-risk mitigations (audit flagged HIGH)

| Mitigation | Where |
|---|---|
| Never skip first-sight | `SkinSlot::has_populated_output: bool` invariant; pinned by `first_sight_pose_is_always_dirty` |
| Same dirty bit drives both dispatch + refit | Single `pose_dirty: &HashSet<EntityId>` parameter to `draw_frame`; same predicate used in both loops |
| LRU not reaped on skip | `slot.last_used_frame = self.frame_counter as u64;` happens BEFORE the skip gate at `draw.rs:921` |
| `clear_pose_dirty` preserves baseline | Pinned by `clear_pose_dirty_preserves_baseline_hash`. Wiping the baseline would re-dirty every entity every frame |
| `sweep` drops stale hashes | Pinned by `sweep_drops_stale_pose_hash_with_slot`. Keeps `last_pose_hash` bounded to live entities |
| Refit gated on live BLAS | `accel.has_skinned_blas(entity_id)` — first-sight entity whose BUILD just landed still falls through to refit (no-op but safe) |

## Files touched (6)

1. `crates/core/src/ecs/resources.rs` — SkinSlotPool extension + 5 unit tests
2. `byroredux/src/render/skinned.rs` — pose_hash helper + 3 unit tests + `try_mark_pose_dirty` call
3. `crates/renderer/src/vulkan/skin_compute.rs` — SkinSlot.has_populated_output field
4. `crates/renderer/src/vulkan/acceleration/blas_skinned.rs` — has_skinned_blas accessor
5. `crates/renderer/src/vulkan/context/draw.rs` — both skip gates, new draw_frame param
6. `byroredux/src/main.rs` — pass `pose_dirty` through to `draw_frame`

6 files = slightly over the Phase 4 "5 files" threshold, but the issue explicitly requires #1195 + #1196 atomic and the work is naturally indivisible.

## Tests

8 new tests pass:
- `first_sight_pose_is_always_dirty`
- `unchanged_pose_is_not_dirty_on_second_call`
- `changed_pose_re_dirties`
- `clear_pose_dirty_preserves_baseline_hash`
- `sweep_drops_stale_pose_hash_with_slot`
- `pose_hash_tests::identical_slices_hash_identically`
- `pose_hash_tests::single_bit_change_changes_hash`
- `pose_hash_tests::empty_slice_yields_offset_basis`

Full workspace: 349/349 core + 336/336 byroredux + 278/278 renderer.

## Measurement gap

Per the issue: "Theoretical upper bound at 60% idle NPC rate: ~0.5–1 ms / frame on Prospector compute side. Combined with PERF-DIM7-02's BLAS refit gate: ~3 ms / frame upper bound."

Measurement requires a bench run with `--bench-frames` + reading `tex.skin` for the actual `dispatches_skipped` count. Not run this session — left as a follow-up bench validation, same as #1132's brd_ms tracking.
