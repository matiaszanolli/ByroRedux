# PERF-SD-2026-10-19-04: `populate_object_graphs` retries a prop whose behaviour graph is unavailable on every streaming frame

Labels: low,performance,animation,bug
URL: https://github.com/matiaszanolli/ByroRedux/issues/6126

**Severity**: LOW · **Dimension**: CPU Hot Paths

## Location
`byroredux/src/app_step.rs` (`populate_object_graphs` called each `step_streaming`), `byroredux/src/asset_provider/object_graph.rs` (`populate_object_graphs`)

## Description
The pass selects entities that carry `BehaviorGraphRef` and lack `ObjectAnimationGraph`. When `load_graph` fails (project/behaviour/skeleton not in the archives, or a decode error) the failure is cached in `ObjectGraphCache` as `None`, but nothing marks the entity, so it stays in `wanting` forever. Each frame such an entity pays a `to_ascii_lowercase`, a `names.join(",")`, a `format!`, a sort, a hash lookup of the cached `None`, then `continue`. Two `Vec`s are also built per frame. The interior path calls the pass once at load, so only exterior streaming loops. `ObjectAnimationGraph::new` also returns `None` when the graph has no machine, a second way to stay unattached.

## Evidence
Code read of the selection and cached-`None` paths; derived cost ~0.3-0.5 us per stuck prop per frame (not measured; the number of such props was not counted).

## Impact
Small per-frame CPU for every graph-less behaviour-graph prop in an exterior session.

## Suggested Fix
Insert a negative marker component (or record the entity in a `HashSet`) when the cached result is `None`, and skip the whole pass while the `BehaviorGraphRef` storage generation is unchanged.

Source: docs/audits/AUDIT_PERFORMANCE_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
