# #486 Investigation

## Current state

`crates/debug-server/src/registration.rs::register_all` lists 17 component types but **does not register `AnimationPlayer` or `AnimationStack`**. Debug snapshots effectively cannot round-trip animation state; the issue's specific claim about `reverse_direction` is a symptom of the broader gap.

## Type checks

All fields on the three animation types are trivially serializable (plain `f32`/`u32`/`bool`/`Vec<_>`/`Option<u32>`), so the fix is a mechanical `#[cfg_attr(feature = "inspect", derive(Serialize, Deserialize))]` derive on each, matching the pattern used throughout `crates/core/src/ecs/components/`.

- `AnimationPlayer` (player.rs:13): `clip_handle: u32`, `local_time: f32`, `playing: bool`, `speed: f32`, `reverse_direction: bool`, `root_entity: Option<EntityId = u32>`, `prev_time: f32`.
- `AnimationLayer` (stack.rs:15): `clip_handle`, `local_time`, `playing`, `speed`, `weight`, `reverse_direction`, `blend_in_remaining`, `blend_in_total`, `blend_out_remaining`, `blend_out_total`, `prev_time` — all `f32`/`u32`/`bool`.
- `AnimationStack` (stack.rs:81): `layers: Vec<AnimationLayer>`, `root_entity: Option<EntityId>`.

`EntityId = u32` so `Option<EntityId>` serialises as `Option<u32>` — no custom impl needed.

## SIBLING coverage

The audit flagged `blend_in_remaining` / `blend_out_remaining` as the same-shape gap. Deriving `Serialize`/`Deserialize` on the whole struct captures every field in one stroke — no per-field bookkeeping needed at the registry. The audit's suggestion to add `reverse_direction` to the `ComponentDescriptor` was the narrow reading; the broader fix (register the whole type) satisfies both the literal ask and the sibling check.

`RootMotionDelta` (root_motion.rs:16) is also an unregistered animation component. Out of scope for this issue — filing as a follow-up if it turns out to matter.

## Fix plan

1. Add `#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]` to `AnimationPlayer`, `AnimationLayer`, `AnimationStack`.
2. Register `AnimationPlayer` and `AnimationStack` in `registration.rs::register_all`.
3. Add a regression test at the player.rs level: round-trip a player that has crossed the reverse boundary and verify `reverse_direction` survives.

## Files

- `crates/core/src/animation/player.rs`
- `crates/core/src/animation/stack.rs`
- `crates/debug-server/src/registration.rs`

Scope: 3 files, well within the 5-file threshold.
