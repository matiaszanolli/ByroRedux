# #3838: PERF-D1-2026-09-05-05: `scene_trigger_actor_approach_system` deep-clones every `ScenePlayer` into a fresh `Vec` each frame

Filed from `docs/audits/AUDIT_PERFORMANCE_2026-09-05.md` (PERF-D1-2026-09-05-05) via `/audit-publish`, 2026-09-05.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3838 --json state`.

---

**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-09-05.md` (PERF-D1-2026-09-05-05), published from `/audit-suite volumetrics-deep`. Premise re-verified against HEAD at publish time.

> Note: `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: CPU Hot Paths
- **Location**: `byroredux/src/systems/cinematic.rs:414-431`; registered unconditionally at `byroredux/src/boot.rs:1020`
- **Status**: NEW
- **Description**: Opens with
  `players.iter().map(|(_, player)| player.clone()).collect()` into a fresh
  `Vec<ScenePlayer>`, then builds two fresh `HashSet`s
  (`HashSet<(u32,u16)>`, `HashSet<u32>`) from it — all per frame, discarded at
  tick end. Registered unconditionally (not env-gated like the M42 AI
  procedures), so it runs in every game/cell; it early-returns when the
  `ScenePlayer` storage doesn't exist, which is the saving grace on
  non-quest-scene content.
- **Evidence**:
```rust
// cinematic.rs:419-424
let players: Vec<byroredux_scripting::ScenePlayer> = {
    let Some(players) = world.query::<byroredux_scripting::ScenePlayer>() else {
        return;
    };
    players.iter().map(|(_, player)| player.clone()).collect()
};
```
  The clone-then-collect exists to release the storage read lock before
  taking `SceneRegistry` — legitimate reason for the copy, not for the fresh
  allocation: the same shape was already fixed with a persistent scratch for
  the AI-package systems under #2033/#3269/#3353.
- **Impact**: One `Vec` allocation + deep clone per running scene per frame,
  plus two `HashSet` allocations, on any cell where a SCEN has ever played.
  Zero cost on content without scenes. The two `HashSet`s are keyed on form
  ids (not a per-entity keyspace), so the #2923 std-hashing rule doesn't
  apply here — this is an allocation finding, not a hashing one.
- **Related**: #2033, #3269, #3353 — same family, same fix pattern.
- **Suggested Fix**: Hoist the `Vec<ScenePlayer>` and the two sets into a
  `make_scene_trigger_actor_approach_system()` closure (the `make_animation_system`
  #1372 pattern), reused via `clear()` + `extend`.
- **Confidence**: High.

### Dimension 3 — GPU Memory Pressure & Eviction Thrash

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
