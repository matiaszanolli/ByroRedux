# Issue #886 (OPEN): INFRA-PERF-01: wire tracing spans for the cell-load critical path — single regression-guard infra for 6+ wall-clock findings

URL: https://github.com/matiaszanolli/ByroRedux/issues/886

---

## Description

The 2026-05-06b performance audit identified six wall-clock findings on the cell-load critical path (#879, #880, #881, #882, #883, #877), none of which has a quantitative regression guard today.

dhat is the **wrong tool** — these are sync fence-wait stalls, ECS lock-acquisition costs, and BSA mutex contention; not allocation count regressions. The right infrastructure is a `tracing` span ladder around the cell-load path, exported via Tracy or a flame-graph dump.

This sits alongside (orthogonal to) the NIF-PERF-* "wire dhat for the NIF parse loop" follow-up from `AUDIT_PERFORMANCE_2026-05-06.md`. Two separate pieces of profiling infrastructure for two different cost axes.

## Scope

Span ladder (top-down) around the cell-load critical path:

```
consume_streaming_payload (byroredux/src/main.rs)
  └─ finish_partial_import (byroredux/src/streaming.rs)
       └─ load_one_exterior_cell (byroredux/src/cell_loader.rs)
            ├─ pre_parse_cell                     ← #877 (BSA mutex contention)
            ├─ load_references                    ← #523 batched lookup
            │    └─ spawn_placed_instances        ← #879 (REFR mesh upload)
            │         ├─ upload_scene_mesh        ← #879 fence-waits per placement
            │         ├─ StringPool intern        ← #882 (per-mesh write lock)
            │         └─ acquire_by_path          ← #881 (texture upload budget)
            ├─ npc_spawn::spawn_npc               ← #880 (NPC NIF re-parse)
            │    └─ load_nif_bytes_with_skeleton  ← #880 (cache bypass)
            └─ unload_cell (when crossing radius_unload)
                 └─ 6× SparseSet scans            ← #883 (collapse to 1 walk)
```

## Why it matters

- Each of #877, #879, #880, #881, #882, #883 ships a fix with "expected savings" prose but no measurement infrastructure.
- A regression that re-introduces (e.g.) per-placement upload churn, or cache-bypass on NPC spawn, wouldn't be caught by `cargo test` — both fixes preserve correctness.
- Tracy / flame-graph from a `tracing` ladder is the standard tooling for exactly this problem. Single piece of infra, six findings backed.

## Proposed Approach

1. Add `tracing` (and `tracing-subscriber` for development output) to workspace deps if not already present
2. `#[tracing::instrument(skip(...))]` macros on the critical-path functions listed above
3. Optional Tracy integration (`tracy-client` crate with `tracing-tracy` bridge) gated behind a feature flag — for visualization without forcing the dep on default builds
4. CI / repro: a `cargo run --features tracing-tracy -- --esm Fallout3.esm --cell Megaton01 --bsa "Fallout - Meshes.bsa" --textures-bsa "Fallout - Textures.bsa"` flow + a Tracy `.tracy` capture file checked in as a baseline (one-time; not for every PR)
5. Per-finding regression test: a documented profile-capture step in each fix PR's "Test plan" until automated capture-diff exists

## Out of Scope

- GPU timestamp queries (separate finding; needs a RenderDoc capture-diff workflow)
- Allocation regression coverage (separate; the dhat-infra gap from `AUDIT_PERFORMANCE_2026-05-06.md`)

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Pair this with the existing dhat-infra gap follow-up so future audits find both pieces of measurement infrastructure together
- [ ] **DROP**: Verify `tracing-tracy` feature gating doesn't change runtime behavior on default builds
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Smoke test — feature-gated `cargo build --features tracing-tracy` succeeds; default build unchanged

## References

- Audit: `docs/audits/AUDIT_PERFORMANCE_2026-05-06b.md` (Profiling-Infrastructure Gap section)
- Findings backed: #879, #880, #881, #882, #883, #877
- Companion: dhat-infra gap from `AUDIT_PERFORMANCE_2026-05-06.md`
