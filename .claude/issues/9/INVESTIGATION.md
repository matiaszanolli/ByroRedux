# Investigation: Issue #9 — Name collision with duplicate node names

## Root Cause
Global NameIndex maps FixedString → EntityId. Multiple entities with the same name
(e.g., "Scene Root" from two different NIFs) overwrite each other. Animation targets
whichever was inserted last.

## Fix: Subtree-scoped name lookup
Now that #2 is done (Parent/Children hierarchy), animation can be scoped:

1. Add `root_entity: Option<EntityId>` to `AnimationPlayer` and `AnimationStack`
2. When loading a NIF with --kf, set root_entity to the first node entity (root of hierarchy)
3. In animation_system: if player has root_entity, build a scoped name→entity map by
   walking the Children hierarchy from root. Otherwise fall back to global NameIndex.
4. Each NIF instance gets its own subtree — no name collision.

## Files
1. `crates/core/src/animation.rs` — add root_entity field to AnimationPlayer + AnimationStack
2. `byroredux/src/main.rs` — subtree walk in animation_system, set root_entity on spawn

**2 files — within threshold.**
