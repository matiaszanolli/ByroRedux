# #2399 — CONC-D3-2026-08-07-01: Animation channel sinks are lock-acquired in NIF-authored channel order, so the acquisition order between six storages is content-determined

- **Severity**: HIGH
- **Domain**: sync, ecs
- **Audit**: `docs/audits/AUDIT_CONCURRENCY_2026-08-07.md`
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2399


- **Severity**: HIGH
- **Dimension**: 3 — ECS Lock Ordering
- **Location**: `byroredux/src/systems/animation.rs:139-221` (`apply_color_channels`), `byroredux/src/systems/animation.rs:247-330` (`apply_float_channels`)
- **Status**: NEW

**Description**

Both helpers lazily acquire one `QueryWrite` per sink on first use (`write_lazy!` → `$cache.get_or_insert_with(|| $world.query_mut::<$Comp>())`) and hold every acquired guard for the rest of the call. Which guard is taken first — and therefore the pairwise acquisition order across the six storages — is decided by the order channels appear in the `AnimationClip`, i.e. by authored NIF/KF content, not by code. `apply_color_channels` can hold up to six guards simultaneously (`AnimatedDiffuseColor`, `AnimatedAmbientColor`, `AnimatedSpecularColor`, `AnimatedEmissiveColor`, `AnimatedShaderColor`, `LightSource`); `apply_float_channels` up to five. This is exactly the situation the TypeId-sort invariant exists to prevent, and it is the only place in the audited surface where a lock order is not fixed at compile time — materially different from the already-tracked "fixed order, safe only by exclusive scheduling" class (#2153/#2154/#2269), since here both acquisition directions genuinely occur within a single frame, driven by content.

**Evidence** (re-confirmed at publish time against commit `79bfc76e`):

Clip A ordered `[Diffuse, Emissive]` records the `AnimatedDiffuseColor → AnimatedEmissiveColor` edge in `lock_tracker::global_order::GRAPH`; clip B ordered `[Emissive, Diffuse]`, processed later in the same `for ps in playback_scratch` loop, then acquires `AnimatedDiffuseColor` while `AnimatedEmissiveColor` is held. `record_and_check` (`lock_tracker.rs:256-270`) tests exactly `new_edges.contains(held_id)` and panics with "ECS cross-thread deadlock risk (ABBA)". The same applies inside `apply_float_channels` for e.g. `[Alpha, UvOffsetU]` vs. `[UvOffsetU, Alpha]`, which is common shader/UV-animation authoring.

```rust
// animation.rs — write_lazy! macro, lazily orders guard acquisition by channel iteration order
macro_rules! write_lazy {
    ($cache:ident, $Comp:ty, $world:expr, $entity:expr, $value:expr) => {{
        let q = $cache.get_or_insert_with(|| $world.query_mut::<$Comp>());
        ...
```

**Impact**

(a) A debug build with `BYRO_LOCK_ORDER_CHECK=1` — set on the `lock-order-check` and `vulkan-validation` CI jobs — aborts the process the first time a cell loads two clips whose channel orders disagree, silently capping the detector's usable coverage at content that happens not to trip it (eroding the guarantee #2137/#2155 were filed to establish). (b) The latent deadlock is real but currently unreachable: `make_animation_system()` is the sole entry in the `Stage::Update` parallel batch (`boot.rs:748`), `animate_lights_system` (the other `LightSource` writer) is `add_exclusive`, and `render/lights.rs` runs main-thread after `scheduler.run`. Adding any second `add_to_with_access` system to `Stage::Update` touching two of these six storages converts (b) into a live hang.

**Trigger Conditions**: (a) Debug build + `BYRO_LOCK_ORDER_CHECK=1` + any loaded content where two `AnimationClip`s (or two entities' clips) list two of the six sinks in opposite order — no concurrency needed, the graph is process-global. (b) True deadlock additionally requires a second thread in the same stage holding one of the pair; not currently reachable.

**Related**: #313 (TypeId-sorted acquisition), #1410/#2137 (detector in CI), #2155 (detector coverage is reachability-bounded — this finding is a concrete instance of the tail it warns about), #1785 (established all six colour sinks as live).

**Suggested Fix**: Make the acquisition order structural rather than data-driven — bucket channels by target in a first pass and acquire the needed sinks in a fixed declared order (the order `boot.rs:753-780` already lists them in), or acquire each sink, drain its channels, and drop it before touching the next.

## Completeness Checks
- [ ] **LOCK_ORDER**: The fix must produce a compile-time-fixed acquisition order across all six (color) / five (float) sinks, not merely reduce the odds of hitting the content-driven ordering
- [ ] **SIBLING**: Check for the same "lazy-acquire in channel-iteration order" shape elsewhere in the animation stack (bool/texture-transform channels, if any hold multiple guards)
- [ ] **TESTS**: A regression test loading two clips with deliberately opposite channel orders (e.g. `[Diffuse, Emissive]` vs `[Emissive, Diffuse]`) under `BYRO_LOCK_ORDER_CHECK`/`set_enabled_for_tests(true)`, asserting no panic after the fix

---
Filed from `docs/audits/AUDIT_CONCURRENCY_2026-08-07.md` via `/audit-publish`.
