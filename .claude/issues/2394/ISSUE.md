# #2394 — ECS-D7-2026-08-07-01: `OneShotSound` marker is never cleared on the audio dispatch failure paths — per-frame retry + unbounded `log::warn`

- **Severity**: MEDIUM
- **Domain**: ecs
- **Audit**: `docs/audits/AUDIT_ECS_2026-08-07.md`
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2394


- **Severity**: MEDIUM
- **Dimension**: 7 — Component Lifecycles (transient marker lifetime)
- **Location**: `crates/audio/src/lib.rs:909-980`
- **Status**: NEW

**Description**

`audio_system`'s one-shot dispatch removes `OneShotSound` only from entities in the `started` vec. Both failure arms (`add_spatial_sub_track` → `Err`, `track.play` → `Err`) `continue` before `started.push(p.entity)`, so the marker survives the frame. The skill's Dimension-7 contract for this marker is "removed once dispatched / no infinite-marker leak"; on these two arms it is neither dispatched nor removed, and the entity is re-collected into `pending` on every subsequent frame.

**Evidence** (verified directly, `crates/audio/src/lib.rs:928-968`; re-confirmed at publish time against the same commit `79bfc76e`):

```rust
let mut track = match mgr.add_spatial_sub_track(listener_id, p.position, track_builder) {
    Ok(t) => t,
    Err(e) => { log::warn!("M44 Phase 3: add_spatial_sub_track failed for entity {:?}: {e}", p.entity); continue; }
};
...
let handle = match track.play(sound) {
    Ok(h) => h,
    Err(e) => { log::warn!("M44 Phase 3: track.play failed for entity {:?}: {e}", p.entity); continue; }
};
...
started.push(p.entity);
// …
if !started.is_empty() {
    if let Some(mut oneshot_q) = world.query_mut::<OneShotSound>() {
        for entity in started { oneshot_q.remove(entity); }   // ← only the successes
    }
}
```

`prune_stopped_sounds` cannot help: it walks `audio_world.active_sounds`, and a failed dispatch never pushed an `ActiveSound`.

**Impact**

Two reachable regimes. (a) *Transient* — kira sub-track resource limit hit during a busy combat/footstep burst: every marked entity re-attempts next frame and emits one `warn!` per entity per frame until a track frees. Realistic log-flood at 60 Hz. (b) *Persistent* — a `track.play` that always fails for a given `StaticSoundData`: the entity holds `OneShotSound` for the rest of the session, and each frame allocates a spatial sub-track (`add_spatial_sub_track` succeeds) that is immediately dropped at the `continue`. That is a per-frame kira allocate/free churn plus an unbounded warn stream. No unbounded memory growth (the marker set is bounded by the tagged entities), which is why this is MEDIUM rather than HIGH.

**Related**: Skill Dimension 7 "`OneShotSound` markers are pruned once kira reaches `PlaybackState::Stopped` — verify no infinite-marker leak." No matching issue in `/tmp/audit/issues.json`.

**Suggested Fix**: Push `p.entity` onto `started` (or a separate `consumed` vec) on both error arms so the marker is dropped regardless of outcome — a failed one-shot is still a consumed one-shot. If retry is deliberate, bound it (attempt counter on the marker) and rate-limit the `warn!`.

## Completeness Checks
- [ ] **SIBLING**: Check other transient-marker consume paths (scripting events, `ScriptTimer`) for the same fail-without-consume shape
- [ ] **TESTS**: A regression test drives a forced `add_spatial_sub_track`/`track.play` failure and asserts `OneShotSound` is removed and no per-frame re-dispatch occurs

---
Filed from `docs/audits/AUDIT_ECS_2026-08-07.md` via `/audit-publish`.
