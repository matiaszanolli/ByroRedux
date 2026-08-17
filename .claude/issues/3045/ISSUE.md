# REN-D9-01: #2923 FxHash conversion stopped one field short

**Issue**: #3045
**Severity**: LOW
**Labels**: `low,renderer,performance,bug`
**Source report**: `docs/audits/AUDIT_RENDERER_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_RENDERER_2026-08-16.md`.

**Location**: `crates/renderer/src/vulkan/context/mod.rs`

## Description

The #2923 hot-path `FxHash` conversion stopped **one field short of its own hot path**. `skin_dispatch_seen_scratch` is still a `std::collections::HashSet<EntityId>` while its siblings in the same struct were converted.

## Evidence

```rust
// crates/renderer/src/vulkan/context/mod.rs (re-verified 2026-08-17)
:1136  previous_rigid_models: FxHashMap<u32, [f32; 16]>,
:1147  current_rigid_models_scratch: FxHashMap<u32, [f32; 16]>,
:1172  skin_dispatch_seen_scratch: std::collections::HashSet<byroredux_core::ecs::storage::EntityId>,
```

The neighbouring fields carry an explicit `#2174 / D2-03 — FxHashMap, not the std default` comment; this one was missed.

## Impact

SipHash on a per-frame, per-entity keyspace — the exact cost #2923 exists to remove. Bounded (one set, per frame) so the impact is small, but it is inside the skinning dispatch path that the hot-path rule names.

## Suggested Fix

Convert to `FxHashSet<EntityId>` and extend the existing `"{what} must stay \`FxHashSet\` (#2923)"` assertion to cover it, so it cannot regress.

## Related

- #2923, #2174, #1368 (the three prior rounds of this defect class)
- #2985 (TD9-2026-08-16-03 — `skin_offsets`, the *other* collection the rule names that has no guard)

## Completeness Checks
- [ ] **SIBLING**: Fixed together with #2985 — both are unconverted/unguarded members of the same rule
- [ ] **GUARDED**: The existing assertion is extended to cover this field
- [ ] **HOT-PATH-ONLY**: Load-time and DoS-facing maps deliberately left on std hashing
- [ ] **TESTS**: Reverting the field to std `HashSet` fails the guard

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3045 --json state` when live state is needed.*
