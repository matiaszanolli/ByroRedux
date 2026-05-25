# #1269 — SAFE-DIM3-NEW-01: NIF scene-graph and collision walkers have no recursion-depth guard

URL: https://github.com/matiaszanolli/ByroRedux/issues/1269
Filed: 2026-05-25
Labels: low, nif-parser, safety, bug

> Snapshot of the issue as filed. GitHub is authoritative for current state.

---

## Source

`/audit-safety 3` sweep — `docs/audits/AUDIT_SAFETY_2026-05-25_DIM3.md` (Dimension 3: Memory Safety, §3.7 Stack overflow risk).

## Locations

- `crates/nif/src/import/walk/mod.rs:165-235` — `walk_node_hierarchical` (self-call at L224)
- `crates/nif/src/import/walk/mod.rs:643-695` — `walk_node_flat` (self-call at L682)
- `crates/nif/src/import/collision.rs:127-180` — `resolve_shape` (self-call via MOPP at L129, ListShape children at L136, TransformShape at L151, Phantom inner at L179)

## Description

Three recursive NIF traversals carry no depth guard. Depth is bounded only by the on-disk NIF block tree. Shipped Bethesda content does not approach a stack-overflow depth, so this is hardening only — but a malformed third-party or adversarial NIF could in principle crash the process via stack overflow rather than being rejected with a parse error.

In particular, a `BhkListShape` whose `sub_shape_refs` form a cycle through another `BhkListShape` would stack-overflow `resolve_shape` rather than return an error.

## Evidence

- `walk_node_hierarchical` self-call at walk/mod.rs:224 with no `depth` parameter or `MAX_DEPTH` constant.
- `walk_node_flat` self-call at walk/mod.rs:682 — same shape, same gap.
- `resolve_shape` in collision.rs has four recursive arms (MOPP, ListShape children, TransformShape child, Phantom inner) — no `visited: HashSet<usize>` for cycle detection.

## Impact

Loading a hostile or corrupted NIF crashes the process (stack overflow → abort) instead of failing the import cleanly. The rest of the NIF parser is otherwise designed to tolerate corrupt data via `Result`-returning paths and per-block graceful recovery (Session 8). This is the one path that doesn't follow that contract.

## Related

- NIF parser graceful-recovery design (Session 8 "graceful per-block parse recovery").
- `#464` confirmed transform propagation is iterative BFS via `VecDeque` — the import-time walkers were not converted.

## Suggested Fix

1. Add an explicit `depth: u32` parameter (or carry it on `inherited_props` length) to both `walk_node_hierarchical` and `walk_node_flat`; bail with a logged warning at `MAX_NIF_NODE_DEPTH = 128` or similar.
2. For `resolve_shape`, pass a `visited: &mut HashSet<usize>` that records `BlockRef` indices already on the stack — return `None` with a log on revisit to break cycles.

## Completeness Checks

- [ ] **UNSAFE**: N/A — no unsafe in the recursive paths.
- [ ] **SIBLING**: Cross-check `crates/nif/src/import/walk/mod.rs` for any other recursive node walkers (currently two — hierarchical and flat). Cross-check `crates/nif/src/import/collision.rs` for any other recursive shape resolvers.
- [ ] **DROP**: N/A — recursion fix touches only parse-time CPU code, no Vulkan resources.
- [ ] **LOCK_ORDER**: N/A — parse-time, single-threaded.
- [ ] **FFI**: N/A.
- [ ] **TESTS**: Add a regression test that constructs a synthetic NIF with a `BhkListShape` cycle and asserts the loader returns an error (or logs and returns `None`) without stack overflow. Add a synthetic NIF with depth-256 node nesting and assert it loads or fails cleanly.
