# #2668 — SCR-D4-NEW11-02: OffsetMap::to_original is an unindexed linear scan over an already-sorted vec, giving O(N*E) error remapping

**Severity**: LOW · **Dimension**: Papyrus Lexer & Pratt Parser
**Location**: `crates/papyrus/src/lexer.rs::OffsetMap::to_original`

## Fix

Verified the premise: `OffsetMap::push` always appends with a strictly
increasing preprocessed offset (`original_offset` only grows across the
`preprocess` scan, `prior_removed` is monotonically non-decreasing, so
`preprocessed_offset = original_offset - prior_removed` never decreases),
confirming `entries` is sorted ascending by `pp_off` by construction.
`to_original` walked the whole vec linearly on every call to find the last
entry with `pp_off <= preprocessed`.

Applied the issue's own suggested fix: replaced the linear scan with
`Vec::partition_point` (binary search over the sorted vec). Also folded in
the linked cleanup (SCR-D4-NEW11-02's own text, "the dead `removed`
accumulator in `preprocess` is a free cleanup to fold in here"): removed
the `removed` local in `preprocess` — it was incremented in three branches
and then discarded via `let _ = removed;`, never actually read.

## TESTS (issue's own checklist item — "a regression test pins this
specific fix")

`to_original_bisects_correctly_across_multiple_continuations` — a
three-continuation fixture (`entries` has 3 entries, not 1 like the only
prior test), asserting `to_original` at every interesting offset: before
any continuation, exactly at each continuation's own preprocessed offset
(the boundary the `<=` predicate must include), strictly between two
continuations, and past the last one. Every expected value was hand-derived
against the actual original-source offsets (not just against the formula),
as a sanity cross-check.

**Reintroduce-and-revert verification**: temporarily changed the
bisection predicate from `pp_off <= preprocessed` to `pp_off < preprocessed`
(an off-by-one that mis-maps every boundary offset) — confirmed the new
test failed at the very first boundary assertion (`'c', first boundary`,
expected 4 got 2). Restored the fix and reran — all 91 tests in
`byroredux-papyrus`'s `lexer::tests` pass again.

## SIBLING (issue's own dimension convention)

Searched the file for other manual scans over `entries` or similarly
sorted structures — `push` (append-only) and the new bisection are the
only two consumers; no other linear scan needed the same treatment.

## Verification

- `cargo check -p byroredux-papyrus --tests`: clean, zero warnings.
- `cargo test -q -p byroredux-papyrus`: 91 + 4 tests passing, 0 failing
  (+1 new).
- `cargo test -q --no-fail-fast` (full workspace): **7160 passing, 0
  failing**.
