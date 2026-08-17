# PERF-D6-01: #2923's Fx-hashing covered 1 of ~9 per-frame per-entity probes

**Issue**: #3061
**Severity**: LOW
**Labels**: `low,renderer,performance,bug`
**Source report**: `docs/audits/AUDIT_PERFORMANCE_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_PERFORMANCE_2026-08-16.md` (Dimension 6 — hot-path hashing).

**Location**: `crates/renderer/src/vulkan/context/mod.rs`:1172, 1201, 1314, 1333, 1353, 1574
**Status note**: NEW — residual of #2923, which is CLOSED and **not** regressed.

## Description

#2923's Fx-hashing conversion covered **1 of ~9 per-frame per-entity probes on the same call path**; the siblings are still SipHash-1-3.

## Evidence

Re-verified 2026-08-17 — six std collections survive on the skin/blend per-frame path:

```
1172: skin_dispatch_seen_scratch: std::collections::HashSet<EntityId>
1201:                             std::collections::HashSet<EntityId>
1314: pub skin_slots:             std::collections::HashMap<…>
1333: pub failed_skin_slots:      std::collections::HashSet<EntityId>
1353: pub failed_skin_blas:       std::collections::HashSet<EntityId>
1574: blend_seen_scratch:         std::collections::HashSet<(u8, u8, bool, bool)>
```

plus `blend_pipeline_cache: HashMap<(u8,u8,bool,bool), …>` at :1567.

All are per-frame scratch or per-entity keyed on the skinning/blend dispatch path — the same access shape that made `pose_dirty` worth converting.

## Impact

SipHash over per-frame per-entity keyspaces, on the render hot path, in the crate the #2923 rule explicitly names. Bounded per frame, so the cost is modest — but it is the unfinished half of a conversion the project has now revisited three times (#1368 → #2174 → #2923).

## Suggested Fix

Convert the six to `FxHashSet`/`FxHashMap` and extend the existing `"{what} must stay \`FxHashSet\` (#2923)"` assertion to cover them, so the cluster cannot drift back a fourth time.

Keep std hashing on anything DoS-facing — none of these are.

## Related

- #2923 (CLOSED, not regressed — this is its residual), #2174, #1368
- #3045 (REN-D9-01 — `skin_dispatch_seen_scratch` specifically) and #2985 (TD9-03 — `skin_offsets`); **all three overlap and should be one pass**

## Completeness Checks
- [ ] **ONE-PASS**: Resolved together with #3045 and #2985 — three findings, one cluster
- [ ] **GUARDED**: The source-text assertion is extended to every converted field
- [ ] **HOT-PATH-ONLY**: Load-time and DoS-facing maps deliberately left on std hashing
- [ ] **TESTS**: Reverting any converted field to std fails the guard

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3061 --json state` when live state is needed.*
