# Investigation — #882 (CELL-PERF-05)

## Domain
ECS + binary (cell_loader.rs)

## Lock sites in `spawn_placed_instances` per-mesh loop

1. **Read lock per mesh** at `byroredux/src/cell_loader.rs:2118`:
   ```rust
   let pool_read = world.resource::<byroredux_core::string::StringPool>();
   ```
   Used to resolve ~10 texture-path slots per mesh (diffuse, normal, glow,
   gloss, parallax, env, env_mask, material, detail, dark). Dropped at
   `:2138`.

2. **Write lock per mesh** at `byroredux/src/cell_loader.rs:2225`:
   ```rust
   let mut pool = world.resource_mut::<byroredux_core::string::StringPool>();
   let sym = pool.intern(name);
   drop(pool);
   ```
   One intern per `mesh.name`.

For Megaton (929 REFRs, hundreds of mesh-bearing) → ~hundreds of read +
~hundreds of write lock acquisitions in a single cell-load loop.

## Other StringPool sites (already batched, no fix needed)

- `cell_loader.rs:976` — load_references resolver setup, once per call.
- `cell_loader.rs:1223` — clip-conversion under cache-miss path, once
  per unique NIF.
- `cell_loader.rs:1676/1680/1689` — finish_partial_import, once per
  unique parsed NIF.

## Constraint discovered

`world.resource_mut::<StringPool>()` borrows `&world`. The per-mesh loop
calls `world.spawn()` and `world.insert()`, both of which require
`&mut world`. So the simple "hoist and hold" pattern fails the borrow
checker — we MUST do the two-phase pattern from the issue:

  Phase 1: single pool lock pre-pass over `imported`, resolving every
  path slot + interning every name into a `Vec<ResolvedMeshPaths>`.

  Phase 2: spawn loop reads pre-computed values without touching the
  pool.

## Sibling-pattern check

Other `spawn_placed_instances`-equivalent paths:
- `cell_loader_terrain.rs` — terrain tile spawn does NOT touch
  StringPool inside its loop (no per-tile names; texture paths are
  resolved once per LTEX layer at the top-level loop). Already batched.
- `scene.rs::load_nif_bytes` — single-NIF render path; one mesh in the
  loop, so per-iteration acquisition is already O(1). No fix.

## In-loop helpers verified safe to keep within phase 2 (no StringPool re-entry)

- `resolve_texture_with_clamp(ctx, …)` — VulkanContext-only.
- `world.spawn()` / `world.insert()` — ECS storage, no resource lock.
- `helpers::add_child(world, parent, child)` — `Children` query only.

So the borrow checker on the new design is OK as long as the pool guard
is dropped BEFORE the spawn loop begins.

## Test approach

Behavior is preserved; the existing 1500+ test suite + cell-load
integration tests already exercise the resolved-path + name-intern
path. Adding a focused regression that "counts lock acquisitions" would
require StringPool instrumentation — out of scope for this fix.
