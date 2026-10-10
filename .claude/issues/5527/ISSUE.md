# #5527: PERF-D1-2026-10-09-01: An "In-Cell" Eat/Sleep idle re-resolves every frame, and the walk leg pays per-actor lock round-trips

**Labels**: ai, bug, gameplay, low, performance

**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-10-09.md` — finding `PERF-D1-2026-10-09-01` · suite `/audit-suite --preset streaming-deep` (2026-10-09) · HEAD `3bcf6c8e8`

- **Severity**: LOW
- **Dimension**: CPU Hot Paths
- **Location**:
  - `byroredux/src/systems/eat_sleep.rs:126-139`: no `EatSleepState` is recorded when `resolve_anchor` returns `None`.
  - `:199-238`: `resolve_anchor` and `cell_is_resident`. Each call allocates a `to_ascii_lowercase` `String` and does a std `HashMap<String, _>` get.
  - `:140-189`: the walk leg, which per actor per frame does `world.get::<EatSleepState>`, `::<GlobalTransform>`, `::<WalkSpeed>` and `::<Transform>`, plus a `PhysicsWorld` `try_resource` and a `query_mut::<Transform>`.
  - Registration: `byroredux/src/boot/schedule/post_update.rs:129` (`add_exclusive`, every frame).
- **Status**: NEW. Arrived with `42aab4c09` (#5391, 2026-10-09). Related: #5449 (open), which covers the same system's arrived-but-unseatable furniture-gather retry. That is a different trigger with the same fix shape.
- **Description**: #5391 gives every Eat/Sleep actor an authored `PLDT` anchor. For an `InCell` package, `resolve_anchor` returns `None` unless the package's cell is the resident interior.
  - In exteriors, In-Cell anchors are never resolved at all (v0): `CurrentCellContext` is removed there (`transition.rs:446`).
  - `None` makes the loop `continue` without recording anything, so the module doc's "resolved again next tick" is literally every frame. That lasts for the whole package time window.
  - Each idle frame pays `world.get::<EatSleepState>` plus `world.get::<GlobalTransform>` plus a resource probe. In a non-matching interior it also allocates the lowercase cell-key `String` and does a SipHash map lookup.
  - Separately, the walk leg reads four components through four individual `world.get` calls. Each one is a full read-lock acquire/release with lock-tracker bookkeeping. #5418 just removed that exact per-entity pattern from the unload detach pass. The other locomotion systems (travel, wander, forcegreet after #5371) gather in query passes instead.
- **Evidence**: `eat_sleep.rs:131` (`let Some(destination) = resolve_anchor(world, npc, location) else { continue; };`) and `:222-238`. The commit's own census says 418 of 706 FNV Eat references use NearEditor, InCell or NearCurrent; it does not give the InCell share.
- **Impact**: a few lock round-trips plus at most one small `String` allocation per affected actor per frame. Bounded by the number of Eat/Sleep actors, unbounded in time. Small. No quantitative guard exists for this site.
- **Related**: #5449, #5418, #3353 (the minute-gate cost model the ambient package system follows), #2033 (the persistent-scratch pattern).
- **Suggested Fix**:
  - Record an "idle until the resident cell changes" state, keyed on the `CurrentCellContext` identity or a load generation, instead of returning `None`.
  - Gather `(EatSleepState, GlobalTransform, WalkSpeed, Transform.rotation)` for all actors in one pass per storage, as `travel.rs` does.
  - Compute the lowercase cell key once per frame. This can share #5449's retry-cadence fix.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other walkers / record decoders / games / call sites)
- [ ] **TESTS**: A regression test pins this specific fix
