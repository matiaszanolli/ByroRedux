# Investigation — #1385 MEM-06 collision shape recursion depth

**Domain:** nif (collision import) / memory

## Root cause
`resolve_shape` (`crates/nif/src/import/collision.rs`) recurses through nested
Havok shapes (BhkListShape, transform/mopp/convex-sweep, convex-list). Cycle
detection (#1269) via a `visited` HashSet catches *repeated* blocks, but a long
*acyclic* chain of distinct single-child shapes (e.g. a corrupt NIF declaring
[0]→[1]→…→[N]) never repeats a block, so it recurses N deep and overflows the
native stack.

## Fix
`visited` holds exactly the blocks currently on the resolution stack (insert on
entry, remove on exit), so `visited.len()` *is* the current depth. Added a
`MAX_COLLISION_SHAPE_DEPTH = 64` cap: right after the cycle insert, if
`visited.len()` exceeds the cap, `warn!` + remove the just-inserted entry (to
keep bookkeeping balanced) + return None. No signature change → all 11 existing
call sites and tests untouched. 64 is far above any real shape tree (MoppBvTree →
List → primitives is ~3 levels).

## SIBLING
Every recursive descent in `resolve_shape_inner` calls back through
`resolve_shape` (never `resolve_shape_inner` directly), so the single gate at the
entry covers all collision recursion paths (transform :367/376, convex-sweep
:406, list :413, convex-list :438, mopp :425).

## Tests
- `deep_acyclic_list_chain_bails_without_overflow` — N = cap+200 chain → None,
  asserts no overflow + balanced `visited` after return.
- `list_chain_within_depth_limit_resolves` — depth-10 chain → terminal Ball,
  proving the cap doesn't regress legitimate nesting.

## Verification
cargo check clean (no warnings in collision.rs); cargo test 2792 passed.

## Completeness
- [x] UNSAFE: N/A · DROP: N/A · SIBLING: single entry gate covers all paths · TESTS: 2 added
