# #6231: GAME-D4-2026-10-21-02: `stand_up` (revive and knockdown get-up) gives every actor the humanoid idle, so a revived or pushed creature stands up with a clip that binds none of its bones

**Labels**: medium,gameplay,combat,animation,bug,game:fo3,game:fnv,game:oblivion

**Source**: docs/audits/AUDIT_GAMEPLAY_2026-10-21.md · **Severity**: MEDIUM · **Dimension**: Combat & Death Pipeline (Dim 4, the reverse death edge) · **Location**: `byroredux/src/systems/revive.rs:102-111` (`stand_up`)

**Audit status**: NEW (from `d1b298921`, 2026-10-18; #6078 `578d8432a` and #6100 `abd3d5001` fixed the spawn paths but not this one)

- **Trigger**: FO3 / FNV / Oblivion. ObScript `Resurrect` / `ResurrectActor` on a creature (`ReviveRequest`, `obscript_effects.rs:599-600`), or a `PushActorAway` / `TrapUpdate` knockdown of a creature that then gets up (`knockdown.rs:187`).
- **Description**: `stand_up` resolves the idle as `humanoid_default_idle_kf_path(game)`, i.e. `_male\locomotion\mtidle.kf` (FO3/FNV) or `_male\idle.kf` (Oblivion), and inserts it on the actor with `with_root(skeleton_root)`. A creature's own idle comes from `creature_idle_kf_paths(game, dir)` beside its skeleton (`npc_spawn.rs:610-626`). The function's own doc says creatures "do not share the humanoid `idle.kf` — a rat's skeleton has none of the humanoid bone names". The ragdoll is freed, but no clip moves the creature's bones out of the pose the ragdoll last wrote. The root `Transform` stands up while the skeleton stays in its collapsed pose, and its re-keyframed bone bodies follow that pose.
- **Evidence**: `revive.rs:102-110`:
  ```rust
  let idle = ... .and_then(|game| {
      crate::npc_spawn::humanoid_default_idle_kf_path(game).and_then(|path| { ...clips.get_by_path(path) })
  });
  ```
  Nothing branches on `ActorBodyClass` or the creature skeleton directory.
- **Impact**: A resurrected or knocked-down creature (FO3/FNV creatures, Oblivion creatures) gets up visibly crumpled and stays that way. The walk take can still animate it while it moves. The humanoid case is also slightly off: the restored idle loses the per-NPC `idle_desync` phase and speed. I did not count how many vanilla scripts resurrect or push creatures.
- **Related**: GAME-D4-01; #6078, #6100.
- **Suggested Fix**: Resolve the idle the way spawn does, through `creature_idle_kf_paths` for a creature skeleton and the humanoid path otherwise. Ideally share the spawn's idle-selection helper so the two cannot diverge again, and re-apply `idle_desync`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (every idle re-insert site (spawn, `stand_up`, save-load revival) resolves the idle through one shared helper)
- [ ] **TESTS**: A regression test pins this specific fix
