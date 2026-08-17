# TD9-2026-08-16-03: skin_offsets — one of the four collections the #2923 hot-path rule names — has no hasher guard

**Issue**: #2985
**Severity**: LOW
**Dimension**: 9 — Test Hygiene (green-by-construction)
**Labels**: `low,renderer,tech-debt,bug`
**Source report**: `docs/audits/AUDIT_TECH_DEBT_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-08-16.md` (Dimension 9 — Test Hygiene, green-by-construction). Effort: trivial.

**Location**: `byroredux/src/main.rs`:158 · `byroredux/src/render/static_meshes.rs`:57 · `byroredux/src/app_frame.rs`:138
**Guards**: `crates/core/src/ecs/resources/skin_slot_pool.rs`:899-963 · `crates/renderer/src/vulkan/context/mod.rs`:4338-4368

## Description

`.claude/commands/_audit-common.md`'s hot-path hashing rule names **four** things that must stay `Fx`-hashed across the crate boundary:

1. every `SkinSlotPool` collection
2. the `pose_dirty` set it hands the renderer
3. `FrameInputs.pose_dirty`
4. *"the `skin_offsets` map threaded through `byroredux/src/render/`"*

The #2923 fix shipped two source-text guards covering the first three — `skin_slot_pool_maps_are_not_siphash` (five fields), `pose_dirty_accessor_does_not_pin_siphash_across_the_crate_boundary`, and `pose_dirty_crosses_the_crate_boundary_without_siphash` (`draw.rs` + `skinned_blas_refit.rs`).

**Nothing pins `skin_offsets`.**

It is `FxHashMap` today at all three sites, so this is a **coverage gap, not a live regression** — but the guards' own doc comment records that this defect class has already recurred three times at three different sites (#1368 → #2174 → #2923, each sweep "missing this cluster entirely"), which is exactly the argument for pinning the fourth.

## Evidence

`byroredux/src/main.rs`:158
```rust
skin_offsets: rustc_hash::FxHashMap<byroredux_core::ecs::EntityId, u32>,
```

`byroredux/src/render/static_meshes.rs`:57 takes `skin_offsets: &FxHashMap<EntityId, u32>` and probes it once per draw at :253 (`skin_offsets.get(&entity)`), inside the static-mesh main loop — **per-frame, per-entity**, the same access shape that made `pose_dirty` worth guarding.

Re-verified 2026-08-16: `grep -rn "2923" byroredux/src` returns exactly one hit — a prose doc comment at `main.rs`:153 explaining the choice. **No assertion.** No `skin_offsets` needle appears in any test in `crates/`.

## Impact

None today. The gap is that the one collection in the rule with **no guard** is the one in the binary crate — which is where the previous two regressions were reintroduced.

## Suggested Fix

Extend `pose_dirty_crosses_the_crate_boundary_without_siphash`'s loop (or add a sibling in `byroredux/src/render/`) to include `include_str!("../main.rs")` and `include_str!("static_meshes.rs")` with the `FxHashMap<EntityId, u32>` needle.

Note the known failure direction of these source-text guards: they pin a fully-qualified spelling while the house style is import-then-bare. That is **safe** — a house-style refactor breaks the positive assertion loudly rather than passing silently — but the new guard should accept both spellings to avoid a false alarm on an unrelated refactor.

## Related

- #1368, #2174, #2923 (the three prior rounds of this defect class)
- The hot-path hashing rule in `.claude/commands/_audit-common.md`:234-246

## Completeness Checks
- [ ] **FOURTH-COLLECTION**: `skin_offsets` pinned at all three sites, not just `main.rs`
- [ ] **RULE-PARITY**: All four collections named by the hot-path rule now have a guard — re-read the rule and confirm none is left
- [ ] **SPELLING-TOLERANT**: The needle accepts both `rustc_hash::FxHashMap` and bare `FxHashMap`
- [ ] **FAILS-LOUDLY**: Swapping one site to `std::collections::HashMap` fails the suite

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state —
query `gh issue view 2985 --json state` when live state is needed.*
