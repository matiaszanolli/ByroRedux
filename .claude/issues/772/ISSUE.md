# Issue #772: LC-D5-NEW-03 — NPC `AnimationPlayer` never attached to spawned skeleton

**Severity**: LOW (intentional Phase 2.x deferral) · **Domain**: animation, legacy-compat · **Type**: enhancement
**Source audit**: docs/audits/AUDIT_LEGACY_COMPAT_2026-04-30.md
**Related**: #771 (paired — closing #771 likely unblocks this); M41.0 Phase 2 commit 35b60cf

## Summary

`byroredux/src/npc_spawn.rs:654-655` discards the idle clip handle and skeleton root entity rather than attaching `AnimationPlayer`. The deferral is documented at lines 622-653: a bind-pose mismatch causes NPCs to vanish when the player ticks against `mtidle.kf` (frame-0 translations don't align with skeleton.nif's authored bind pose).

Compare `byroredux/src/cell_loader.rs:2193-2194` for non-NPC placements where the `AnimationPlayer` attach works end-to-end.

## Game Impact

FNV / FO3 / Oblivion NPCs spawn at correct positions in bind pose but don't idle-breathe. Skyrim+ is bind-pose-only by design until Phase 6 Havok stub.

## Suggested Fix

Pair with #771. If correcting the palette formula resolves the bind-pose mismatch, this becomes a one-line addition (replace the two `let _ =` with `world.insert(skel_root, AnimationPlayer::new(handle).with_root(skel_root))`).

If the gap is real after #771, trace KF channel-root resolution through the merged `node_by_name` map and expand it as needed.
