# LC-D1-NEW-01: SceneFlags inserted by loose-NIF loader but dropped at cell-loader spawn boundary

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/1235
**Filed from**: `docs/audits/AUDIT_LEGACY_COMPAT_2026-05-23.md`
**Severity**: LOW
**Labels**: `low`, `ecs`, `legacy-compat`, `bug`

## Description

`SceneFlags` exists at `crates/core/src/ecs/components/scene_flags.rs` with bits for `APP_CULLED` / `SELECTIVE_UPDATE` / `SELECTIVE_XFORMS` / `SELECTIVE_PROP_CONTROLLER` / `SELECTIVE_RIGID` / `DISPLAY_OBJECT` / `DISABLE_SORTING` / `SELECTIVE_XFORMS_OVERRIDE` / `IS_NODE`. `ImportedNode.flags` and `ImportedMesh.flags` both carry the raw `NiAVObject.flags` value through the importer per #222.

The **loose-NIF loader** (`load_nif_bytes_with_skeleton`) inserts the component on both NiNode and mesh entities:
- `byroredux/src/scene/nif_loader.rs:450-452` — NiNode path
- `byroredux/src/scene/nif_loader.rs:789-791` — mesh path

The **cell-loader spawn path** — dominant entry point for every cell-loaded entity (Megaton, Diamond City, every grid-loaded exterior REFR) — never inserts `SceneFlags`. `mesh.flags` is read exactly once across the entire cell-loader subtree (a `LightData.flags` copy at `byroredux/src/cell_loader/spawn.rs:980`, unrelated to `SceneFlags`); `ImportedNode.flags` is never read at all.

This is the last sibling of the D1-NEW-01..03 cluster (#1212 / #1213 / #1214) that was not picked up by the 2026-05-19 closure — same shape: parsed data dropped at the cell-loader spawn boundary.

## Evidence

```
$ grep -rn "SceneFlags" byroredux/src/cell_loader/
(no results)

$ grep -c "SceneFlags" byroredux/src/scene/nif_loader.rs
3
```

The loose-NIF path guards on `flags != 0` to avoid empty rows:
```rust
// nif_loader.rs:789-791
if mesh.flags != 0 {
    world.insert(entity, SceneFlags::from_nif(mesh.flags));
}
```

## Impact

- **Functional, today**: nil. No runtime system reads `SceneFlags` post-spawn — `APP_CULLED` is already filtered at the importer walker (`crates/nif/src/import/walk/mod.rs:344` + `:388` + `:789` + `:815`), so culled shapes never reach the spawn site. The other bits have no consumer yet.
- **Debug, today**: console commands that introspect ECS rows (`inspect`, `prid`) see `SceneFlags` on loose-NIF-loaded entities but never on cell-loaded ones — confusing inconsistency when debugging cell content. Same shape as the dead-`prid`-on-cell case that motivated #1212.
- **Forward-looking**: any future system that toggles visibility (Papyrus `ObjectReference::Disable()`), respects `DISABLE_SORTING` (alpha-stack draw order), or branches on `SELECTIVE_UPDATE` for animation-cost gating will need the row.

## Related

- Closure cluster: #1212 (FormIdComponent) / #1213 (LocalBound) / #1214 (BSXFlags). Same pattern.
- Parent issue #222 — closed when the loose-NIF path landed; cell-loader path was never wired alongside it.

## Suggested Fix

At `byroredux/src/cell_loader/spawn.rs:725` (the per-mesh insert block, alongside `LocalBound::new`), add:

```rust
if mesh.flags != 0 {
    world.insert(entity, SceneFlags::from_nif(mesh.flags));
}
```

For placement-root parity: thread `ImportedNode.flags` (or just the root node's flags) through `CachedNifImport` (same pattern as `bsx_flags` post-#1214) so the root-node bits land on `placement_root`. Loose-NIF parity tests in `byroredux/src/scene/nif_loader_tests.rs` already exist; mirror them under `cell_loader/spawn_tests.rs`.

## Completeness Checks

- [ ] **UNSAFE**: N/A (no `unsafe` in fix).
- [ ] **SIBLING**: confirm no third entry-point (NPC spawn `npc_spawn.rs`, M40 streaming partial `cell_loader/partial.rs`) needs the same wiring. `partial.rs` already plumbs `bsx_flags` from cache — same site, low effort.
- [ ] **DROP**: N/A (no Vulkan objects).
- [ ] **LOCK_ORDER**: per-entity `world.insert` shares the same lock-acquisition shape as the existing `LocalBound` insert at the same site.
- [ ] **FFI**: N/A.
- [ ] **TESTS**: integration test that loads a fixture cell (e.g. via `byroredux/src/cell_loader/spawn_tests.rs`) asserts at least one `SceneFlags` row exists on a mesh entity, mirroring the LocalBound regression test from #1213.
