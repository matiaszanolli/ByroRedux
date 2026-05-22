**Severity**: HIGH
**Dimension**: Scene Graph Decomposition
**Source**: `docs/audits/AUDIT_LEGACY_COMPAT_2026-05-19.md` D1-NEW-01

`FormIdComponent` exists at `crates/core/src/ecs/components/form_id.rs` (re-exported from `mod.rs:39`) and is the backing storage for `World::find_by_form_id`. Today the cell-loader **never inserts it** on any spawned mesh / placement-root / light / particle / collision entity:

```
$ grep -rn "FormIdComponent" byroredux/src/
(no results)
```

#1188 widened `PlacedRef` to carry both `form_id` (placement identity) and `base_form_id` (referenced base record), but the spawn site never reads either into an ECS row.

### Evidence
- `byroredux/src/cell_loader/spawn.rs:142-153` inserts Transform / GlobalTransform / Billboard on the placement root. No `FormIdComponent`.
- `byroredux/src/cell_loader/spawn.rs:636-846` inserts MeshHandle / TextureHandle / Material / etc. on each mesh entity. No `FormIdComponent`.
- `byroredux/src/cell_loader/references.rs:188-237` reads both form-ids for the absorption check but doesn't pass them into spawn.

### Impact
- `World::find_by_form_id(fid)` returns `None` for every REFR loaded by the cell loader.
- `prid <fid>` console command is dead on cell-loaded content.
- Debug-server's "inspect by formid" path is dead.
- Future Papyrus-script ECS adapter resolving `ObjectReference` by formid hits the same dead path.
- Quest / story-manager systems firing on `OnActivate(<fid>)` markers can't locate the target entity.

### Suggested Fix
Insert `FormIdComponent(placed_ref.form_id)` on the placement root in `spawn_placed_instances`. Pass `placed_ref` (or just the two form-ids) through the call signature from `load_references`.

### Completeness Checks
- [ ] **UNSAFE**: N/A — no unsafe in this fix.
- [ ] **SIBLING**: Same pattern checked in `scene/nif_loader.rs` (loose-NIF spawn site) and `cell_loader/precombined.rs` (new today).
- [ ] **DROP**: N/A — no Vulkan objects.
- [ ] **LOCK_ORDER**: N/A.
- [ ] **FFI**: N/A.
- [ ] **TESTS**: Integration test that loads a fixture cell and asserts `world.find_by_form_id(known_refr_fid).is_some()`.
