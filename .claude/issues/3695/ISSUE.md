# #3695 — ECS-D1-01: scene_centroid_distance inverts the canonical GlobalTransform → MeshHandle order

**Severity**: MEDIUM · **Dimension**: Lock Ordering & Deadlock
**Location**: `byroredux/src/app_step.rs` (`App::scene_centroid_distance`); opposing edge at `byroredux/src/render/static_meshes.rs`

## Fix

Swapped the two acquisitions in `scene_centroid_distance` so
`GlobalTransform` is taken first, matching `render/static_meshes.rs`'s
static-mesh pass and the canonical order `docs/engine/ecs.md` fixes for
this cluster. One-line reorder per the issue's own suggested fix, no
behavior difference — the loop body (`for (entity, _) in meshes.iter()`,
`globals.get(entity)`) reads both queries identically regardless of
acquisition order.

## SIBLING (issue's own checklist item)

Swept every `query::<MeshHandle>()` / `query_mut::<MeshHandle>()` call
site in the tree for the same inverted pair. Only `render/static_meshes.rs`
(canonical) and the now-fixed `app_step.rs` combine `MeshHandle` with
`GlobalTransform` in one function; the rest (`ownership_sample.rs`,
`npc_spawn.rs`, `commands/world_info.rs`, `cell_loader/unload.rs`) never
acquire `GlobalTransform` alongside `MeshHandle`, so there's no inversion
risk there. `crates/debug-server/src/evaluator.rs` acquires both, but
already in the correct order (`Transform, Parent, Children,
GlobalTransform, SkinnedMesh, MeshHandle, Name` — an exact match to the
`docs/engine/ecs.md` chain) — no fix needed.

## LOCK_ORDER (issue's own checklist item)

No `RwLock` scope changed — both `let` bindings are held for the same
span as before (until the end of the function), only their relative
acquisition order swapped.

## TESTS (issue's own checklist item)

`scene_centroid_distance` is a private method on `App`, which needs a
real Vulkan device to construct — not unit-testable live, the same
"`cargo test` cannot induce this" situation this file's own
`upscaler_switch_failure_exits_the_event_loop` test already documents and
handles via a static source scan. Added
`scene_centroid_distance_acquires_global_transform_before_mesh_handle`
following that exact precedent: scopes to the function body (start of
`fn scene_centroid_distance` to its closing brace) and asserts the
`self.world.query::<GlobalTransform>()` text position precedes
`self.world.query::<MeshHandle>()`'s.

Verified the guard actually catches a regression (this session's
established quality bar): reverted the acquisition order back to
`MeshHandle` first, reran — the test failed with the exact expected
message, then restored the fix and confirmed a clean pass again.

## Verification

- `cargo check -p byroredux --tests`: clean.
- `cargo test -q -p byroredux --bin byroredux`: 1,869 tests passing, 0
  failing (+1 new).
- `cargo test -q --no-fail-fast` (full workspace): **7090 passing, 0
  failing**.
