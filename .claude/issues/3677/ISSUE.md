# #3677 — PERF-D1-2026-08-30-01: the live animation path is the last unconverted per-frame per-entity SipHash keyspace — `AnimationClip.channels`, `NameIndex.map` and `SubtreeCache.map` are all `std::collections::HashMap`

- **Source**: `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md`
- **Finding ID**: `PERF-D1-2026-08-30-01`
- **Filed**: 2026-08-30 (HEAD `64f64480`)
- **Labels**: low,performance,animation,ecs,bug
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3677

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is authoritative for current state.

---

- **Severity**: LOW
- **Dimension**: CPU Hot Paths
- **Location**: `crates/core/src/animation/types.rs:238` (`pub channels: HashMap<FixedString, TransformChannel>`, `use std::collections::HashMap` at `:5`); `byroredux/src/components.rs:1287` (`NameIndex.map`) and `:1298` (`SubtreeCache.map`) — both reached through `use std::collections::{HashMap, ...}` at `byroredux/src/components.rs:13`, while `rustc_hash::FxHashMap` is *already imported* one line above at `:12`. Probe sites: `byroredux/src/systems/animation.rs:681-690` (`scoped_map` / `resolve_entity`) and `:699` (`for (channel_name, channel) in &clip.channels`).
- **Status**: NEW — not a regression of #2923/#3051/#3061. Those closed the *renderer/skinning* cluster (`SkinSlotPool`, `skin_offsets`, `pose_dirty`, `skin_slots`, `morph_slots`, `blend_pipeline_cache`); the animation-system trio was never in scope and is not covered by the `context/mod.rs:2889` guard, which only pins renderer-owned fields. No open or closed issue names these three.
- **Description**: The project has decided three times (#1368, #2174, #2923/#3061) that std's SipHash-1-3 is the wrong hasher for a per-frame per-entity keyspace, and `_audit-common.md`'s "Hot-path hashing" rule records it as doctrine. The animation player path — the one that actually runs on live game data — probes std maps once per animated *channel* per animated *entity* per frame, at three layers:
  1. `for (channel_name, channel) in &clip.channels` iterates a std `HashMap` (random bucket order, poor locality) once per player entity per frame;
  2. `resolve_entity(channel_name)` → `scoped.get(sym)` on `SubtreeCache`'s inner `HashMap<FixedString, EntityId>`, or `name_index.map.get(sym)`, once per channel;
  3. `apply_float_channels` / `apply_color_channels` / `apply_bool_channels` / `apply_texture_flip_channels` (`animation.rs:766-793`) each call the same `resolve_entity` again per channel they own.

  The key is `FixedString` = `string_interner::DefaultSymbol` (`crates/core/src/string/mod.rs:18`), i.e. a 4-byte integer — the exact input shape where SipHash-1-3's fixed setup/finalisation dominates and `FxHash` wins by the largest factor.
- **Evidence**:
```rust
// crates/core/src/animation/types.rs:238
pub channels: HashMap<FixedString, TransformChannel>,     // std ⇒ SipHash-1-3

// byroredux/src/components.rs:1287,1298
pub(crate) struct NameIndex   { pub(crate) map: HashMap<FixedString, EntityId>, … }
pub(crate) struct SubtreeCache{ pub(crate) map: HashMap<EntityId, HashMap<FixedString, EntityId>>, … }

// byroredux/src/systems/animation.rs:684-690, then :699
let resolve_entity = |sym: &FixedString| -> Option<EntityId> {
    if let Some(scoped) = scoped_map { scoped.get(sym).copied() }   // SipHash
    else { name_index.map.get(sym).copied() }                        // SipHash
};
…
for (channel_name, channel) in &clip.channels {                      // std HashMap iteration
    let Some(target_entity) = resolve_entity(channel_name) else { continue; };
```
- **Impact**: One SipHash round per bone channel per animated actor per frame, plus the same again for every float/colour/bool/flipbook channel the clip carries. Magnitude is bounded by the actor population, for which the repo's own checked-in baselines are the honest citation: `skin_pool_live` = 206 (`.claude/audit-baselines/runtime/fnv-FreesideAtomicWrangler.tsv`), 248 (`fo4-InstituteBioScience.tsv`), 83 (`skyrim_se-WhiterunDragonsreach.tsv`). **The per-channel count is not recorded anywhere in the repo and I did not measure it — that multiplier is unknown.** No `dhat` or timing guard covers this site; `log_stats_system`'s `cpu_ms:` line (`byroredux/src/systems/debug.rs:206`) has no animation bracket, so a regression here is currently invisible to every checked-in instrument.
- **Related**: #2923, #3051, #3061 (the renderer half of the same doctrine, all CLOSED); #1368, #2174; `_audit-common.md` "Hot-path hashing (#2923)".
- **Suggested Fix**: Switch the three declarations to `rustc_hash::FxHashMap` (`rustc_hash` is already a dep of both `crates/core` and `byroredux`, and `FxHashMap` is already imported at `byroredux/src/components.rs:12`). `AnimationClip.channels` is a public field, so pair it with a type alias or fix the handful of construction sites. Then extend the `context/mod.rs:2889`-style source-scan assertion to cover these three so the conversion cannot silently revert.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md` (HEAD `64f64480`). Report status: NEW; re-verified CONFIRMED against HEAD at publish time.*
