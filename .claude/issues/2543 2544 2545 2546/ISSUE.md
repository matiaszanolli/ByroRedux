# Issue batch: 2543, 2544, 2545, 2546

## #2543 — SAFE-2026-08-07-01 (HIGH, safety)
`synthesize_packed_havok_proxy` can build an unbounded/infinite collider from
unclamped REFR scale; the only guard is a `debug_assert!` compiled out of
release builds.

- `byroredux/src/cell_loader/spawn.rs:90-193` (`transformed_mesh_aabb`,
  `synthesize_packed_havok_proxy`, `spawn_packed_havok_proxy`)
- consumed at `crates/physics/src/convert.rs:117-133` (`flatten_to_parts`,
  `CollisionShape::Cuboid` arm)

Fix: reject/clamp non-finite or oversized `half_extents` in
`synthesize_packed_havok_proxy` (mirror `finite_vec`-style pattern used by
`BhkBoxShape` at `crates/nif/src/import/collision/shape.rs:139-150`), and
promote the `convert.rs` `debug_assert!` to a real runtime clamp.

## #2544 — SAFE-2026-08-07-02 (MEDIUM, safety/cxx)
`crates/fsr3-sys/examples/vulkan_context_smoke.rs` — 20 of 23 unsafe
blocks/fns carry no `SAFETY:` comment. Opt-in smoke-test example, not linked
into engine/tests. Fix: add per-call `// SAFETY:` comments or widen the
existing blanket comment on `run()`/`create_and_destroy_context()`.

## #2545 — SAFE-2026-08-07-04 (LOW, documentation/renderer)
`byroredux/src/cell_loader/unload.rs:265-282` (`finish_unload_batch`) — the
unsafe block's comment lost its `SAFETY:` tag during the unload-batching
refactor. One-line fix: re-prefix with `// SAFETY:` and restate the
same-device/allocator precondition.

## #2546 — SAFE-2026-08-07-06 (LOW, documentation)
`.claude/commands/audit-safety/SKILL.md:225-227` — Dimension-7 text
misdescribes the #789 glass-passthrough guard as "texture-equality identity
check"; it's now `materialKind == MATERIAL_KIND_GLASS`
(`crates/renderer/shaders/triangle.frag:1711`). Doc-only fix.
