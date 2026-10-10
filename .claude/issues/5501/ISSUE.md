# #5501: GAME-D5-2026-10-09-03: The stream-boundary snapshot rebuilds `Seated` as a bare marker, without the sit pose, seat rotation or `SeatReservations` entry, so a seated NPC comes back standing in its chair and the seat can be double-booked

**Labels**: ai, bug, gameplay, medium

**Source**: `docs/audits/AUDIT_GAMEPLAY_2026-10-09.md` — finding `GAME-D5-2026-10-09-03` · suite `/audit-suite --preset streaming-deep` (2026-10-09) · HEAD `3bcf6c8e8`

- **Severity**: MEDIUM
- **Dimension**: Dim 5 / Dim 7 (stream-boundary state continuity)
- **Location**: `byroredux/src/cell_loader/stream_snapshot.rs:301-389` (`restore_actor_snapshot`; `Seated` rebuilt at `:367`, translation-only restore at `:319`); called from `byroredux/src/cell_loader/references/synth_child.rs:91`, captured at `byroredux/src/cell_loader/unload.rs:344`; seating write path at `byroredux/src/systems/sandbox.rs:192-245`; reservation prune at `byroredux/src/cell_loader/references/mod.rs:1078-1098`
- **Status**: NEW. The code predates the baseline (EX-16 / #3299, unchanged since `99933f87b`), but it sits in this suite's streaming area and no issue covers it.
- **Trigger**: FO3/FNV exterior. A sandboxing or Eat/Sleep actor seated in a tile that radius streaming evicts and then reloads. It also triggers on interior → interior → back door walks, because the snapshot store is cleared only by `drain_streaming_state` and save reloads.
- **Description**: `apply_seat_assignments` is the only place that really seats an actor. It snaps the root's translation *and rotation* to the marker, parks the sit-enter clip's final frame (`playing = false`), and the caller reserves `(furniture, marker_idx)`. The snapshot carries only `seated_furniture_form_id` and `seated_animation_restore`. On respawn it writes `Seated { furniture, animation_restore }` and the translation, and nothing else:
  - The freshly spawned `AnimationPlayer` stays on the standing idle, and the root keeps the authored REFR rotation. The doc's §4 keep-set says "live position/orientation", but only the position is carried.
  - `Seated` is the one-shot "done" guard for `sandbox_seat_system` and `eat_sleep_system`, so neither ever re-seats the actor.
  - The eviction dropped the old claimant's reservation (`prune_seat_reservations` keeps only live claimant ↔ furniture pairs). The marker index is not in the snapshot, so the restore cannot re-reserve it either. `pick_nearest_seat` consults reservations only, so a second actor can be assigned the same marker and snapped on top of the first.

  The design doc deliberately drops "animation-phase/pose state" on the premise that "a fresh spawn re-entering its default pose for whatever package it resumes into" is harmless. Restoring `Seated` is exactly what stops the package from re-entering its pose.
- **Evidence**:
  ```rust
  seats.insert(entity, Seated { furniture, animation_restore });   // stream_snapshot.rs:367 — no park, no rotation, no reservation
  ```
  For contrast, `apply_seat_assignments`: `t.rotation = seat.rotation; … p.clip_handle = sit_handle; p.playing = false;` and `reservations.0.insert(seat_id, npc)` in its callers.
- **Impact**: After a tile round trip, seated NPCs stand upright inside their chairs, beds or benches, facing their editor direction, until their package next changes. Another sandboxer or diner can take the same seat, leaving two actors snapped to one marker. A save load does not show this, because `Seated` is excluded from the live overlay and the actor re-seats normally. Only the streaming and door paths are affected.
- **Related**: #3299 (closed, origin), #2392 / #2147 (reservation lifetime), #5458 (open: `Seated` allowlist claims).
- **Suggested Fix**: Either drop the `Seated` restore and let the package's own system re-seat the actor (the furniture is resident by construction when the restore resolves it), or carry `marker_idx` and the seat transform in the snapshot and restore through `apply_seat_assignments`, which also handles the reservation. In both cases, carry the rotation the doc already promises.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other walkers / record decoders / games / call sites)
- [ ] **TESTS**: A regression test pins this specific fix
