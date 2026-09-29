# #4991: CONC-D3-2026-09-28-02: `HiddenFirstPerson` is a third render-skip sink that ecs.md does not name and no detector test can see

- **Repo**: matiaszanolli/ByroRedux
- **Labels**: low,concurrency,ecs,test-gap,bug
- **Filed from**: docs/audits/AUDIT_CONCURRENCY_2026-09-28.md
- **Filed**: 2026-09-28

- **Severity**: LOW. The code is correct today. This is a coverage and documentation gap on a
  pattern that went HIGH (#4983) six days ago.
- **Dimension**: ECS Lock Ordering
- **Location**:
  - `byroredux/src/player_body.rs:338-366` (`set_player_view`)
  - `byroredux/src/render/skinned.rs:90`
  - `byroredux/src/render/static_meshes.rs:363`
  - `docs/engine/ecs.md:618-623`
- **Status**: NEW. Introduced by a070baaad (today).
- **Description**: The P3 player body adds `HiddenFirstPerson`. Both render skips read it under
  `GlobalTransform` / `SkinnedMesh`, exactly like `PickedUp` and `NpcAppearanceHidden`.
  `set_player_view` currently gets the order right: it walks `mesh_entities_under` (`Children`,
  `MeshHandle`) at `:346`, before the marker write at `:347`. Two things keep that from being
  guarded:
  - `ecs.md:618` names only `PickedUp` and `NpcAppearanceHidden` as sinks.
  - Nothing registers the storage (`grep register::<…HiddenFirstPerson>` finds no hits; it is
    created lazily by `world.insert` in `attach_assembled_root`). So no render test ever takes
    the `X → HiddenFirstPerson` guard, and the process-wide graph never learns the render half of
    the cycle.

  #4983 was pinned with detector-gated replay tests. This marker has no such test.
- **Evidence**: The hazard mirrors #4983 exactly. Moving the walk inside
  `if let Some(mut hidden_q) = world.query_mut::<HiddenFirstPerson>()` would record
  `HiddenFirstPerson → Children`. Together with `Children → GlobalTransform` (propagation) and
  `GlobalTransform → … → HiddenFirstPerson` (`skinned.rs:81-90`), that closes
  `HiddenFirstPerson → Children → GlobalTransform → HiddenFirstPerson`. A future equip-restamp
  path is the likeliest place for such an edit: newly equipped part meshes currently receive no
  `HiddenFirstPerson`.
- **Trigger Conditions**: Latent. There is no live cycle today.
  - `set_player_view` runs from the winit V-key handler (`app_events.rs:539`, `input` dropped
    first).
  - It also runs from `player.view` via the exclusive debug drain.
  - The render passes run on the main thread outside the scheduler.
- **Impact**: A regression would be invisible to the lock-order lane, the only mechanical guard
  for this class.
- **Verification Path**: Add a `BYRO_LOCK_ORDER_CHECK`-gated test that does three things:
  1. registers `HiddenFirstPerson`;
  2. runs `build_skinned_palettes` / `collect_static_mesh_draws` and transform propagation on a
     world with a stamped body;
  3. then calls `set_player_view`.

  This follows the #4983 pin pattern. It goes red if the walk moves under the guard.
- **Related**: #4983 (closed), #4571, ECS-2026-09-28-D1-02.
- **Suggested Fix**:
  - Add `HiddenFirstPerson` to the ecs.md sink sentence.
  - Register the storage at boot (`boot/world.rs`, beside `PickedUp`). This also removes the
    `query_mut → None` special case noted at `player_body.rs:285-287`.
  - Add the replay test above.

Source: `docs/audits/AUDIT_CONCURRENCY_2026-09-28.md` (CONC-D3-2026-09-28-02) · HEAD `9e6f08870`

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related systems / CI steps
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition and the `docs/engine/ecs.md` canonical order are preserved
- [ ] **TESTS**: A regression test pins this specific fix
