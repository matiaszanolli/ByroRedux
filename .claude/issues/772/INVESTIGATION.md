# Investigation log — #772 NPC AnimationPlayer attach

## Static analysis ruling

Working through each of the three hypotheses listed in the deferral
comment at `byroredux/src/npc_spawn.rs:618-676`:

### Hypothesis 3 — KF channel-root scoping (mostly ruled out statically)

**Setup verified**: `npc_spawn.rs` already mitigates the duplicate-name
collision risk that prompted the deferral.

- Body NIF is parented to `placement_root`, not `skel_root`
  (`npc_spawn.rs:442-443`, intent documented at `:423-441`).
- Head NIF is parented to `placement_root`, not `skel_root`
  (`npc_spawn.rs:591-592`, intent documented at `:584-590`).
- `AnimationPlayer.with_root(skel_root)` scopes the BFS subtree map to
  the skeleton hierarchy alone (`npc_spawn.rs:682`).

Result: channel-root resolution from the skeleton subtree should not
collide with body/head cosmetic copies of `Bip01 Spine` / `Bip01
Head`. The "first vs last visit wins" concern is moot when the
skeleton's bone names are unique inside `skel_root`'s subtree.

**Caveat**: `build_subtree_name_map` (`anim_convert.rs:15-48`) is named
"BFS" but uses `Vec::pop()`, which is LIFO/DFS. With unique names
under `skel_root` this doesn't matter. If a future refactor adds a
duplicate-named entity inside the skeleton subtree, the resolution
order will be DFS-last-wins.

### Hypothesis 1 — KF stores deltas vs absolute (cannot rule out statically)

The system unconditionally writes `transform.translation = pos`
(`systems.rs:374`). If the KF channel is delta-relative-to-bind, the
apply collapses the bone to the delta value (likely small, near-zero).

Looking at `import_kf` (`crates/nif/src/anim.rs`), translation keys are
read at face value via `zup_to_yup_pos(k.value)` and stored as
absolute bone-local translations. The expectation is that the KF
authoring tool writes absolute bone-local poses, matching the
bind-pose convention. **Verifying this requires runtime data**: dump
frame-0 of `mtidle.kf` for a known bone (e.g. `Bip01 Spine`) and
compare against the same bone's bind-pose translation in
`skeleton.nif`.

### Hypothesis 2 — Coord-frame divergence (cannot rule out statically)

Both paths route through `zup_to_yup_pos` so the convention should
agree. Confirmed:

- Bind pose: `crates/nif/src/import/transform.rs` converts NiTransform
  via the standard pipeline.
- KF translation: `crates/nif/src/anim.rs:1764`, `:1335-1346`, `:1044`
  all use `zup_to_yup_pos(k.value)`.

A subtler divergence (e.g. KF channel translations are authored in
the parent's local space while bind-pose translations are in the
bone's local space, or vice versa) cannot be ruled out from static
read alone. Runtime data needed.

## Working-vs-experimental code-path divergence (potentially meaningful)

The "working" comparison sites cited by the audit don't attach the
AnimationPlayer the same way the experimental NPC path does.

- **Working** (`cell_loader.rs:2223-2229`,
  `scene.rs:829-834`): spawn a *new* empty entity dedicated to the
  player, set `player.root_entity = placement_root` (or `nif_root`),
  insert `AnimationPlayer` on the new entity. The player entity has
  no `Transform`, no `Name`, no other components.

- **Experimental** (`npc_spawn.rs:686`):
  `world.insert(placement_root, player)`. `placement_root` already has
  `Transform`, `GlobalTransform`, `Name(editor_id)`, and is the parent
  of the skeleton subtree.

The animation system reads `player.root_entity` and modifies the
transforms of channel-resolved entities; it does *not* modify the
player-bearing entity's transform. So the two patterns *should* be
equivalent. But the cell_loader pattern is the documented working
path; the divergence is worth eliminating before deeper runtime
diagnosis.

**Suggested first runtime experiment**: change `npc_spawn.rs:686` to
match the cell_loader pattern (spawn separate player entity, set
`player.root_entity = skel_root`, insert on the new entity). If NPCs
no longer vanish, the bug was in the player-on-placement_root
composition. If they still vanish, hypotheses 1 or 2 stand.

## #771 ground-truth confirmation (palette formula, ruled out)

`#771` closed without a math change — `palette = bone_world ×
bind_inverse[i]` matches nifly's documented `Skin.hpp:49-51`
skin→bone semantics. So the vanish symptom is *not* in palette
composition; it's strictly a bind-pose / KF-channel mismatch.

## Diagnostic procedure (runtime, requires user run)

The current experimental path (`BYRO_NPC_ANIMATION_EXPERIMENT=1`) emits
a per-NPC `log::warn!` summary when `AnimationPlayer` attaches. To
diagnose the remaining hypotheses, additional per-channel diagnostics
are needed:

1. On first tick, log the channel resolution table:
   `channel_name → resolved_entity → bind_pose_translation →
   frame_0_translation` for the first ~10 channels.
2. Compare the two translation values. Equality (within float epsilon)
   confirms hypothesis 1 (deltas) and hypothesis 2 (coord frame) are
   both ruled out — the apply is a no-op and vanish must come from
   elsewhere (the working-vs-experimental divergence above, or a
   subsequent system that runs in the same frame).
3. Mismatch reveals which axis(es) diverge — diagnostic to whichever
   hypothesis the diff pattern matches.

## Recommended next step

Try the cell_loader-pattern fix (separate player entity) as a one-line
change. If it resolves the vanish, ship #772 closure. If not, the
runtime diagnostic capture is the next step before any further code
change.
