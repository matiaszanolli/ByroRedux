# #3619 — OBL-D3-06: SBSP (33) and ROAD (2) have no code references anywhere

**Severity**: LOW · **Dimension**: ESM Record Coverage
**Location**: `crates/plugin/src/record.rs`, `crates/plugin/src/esm/records/mod.rs`

## Fix

Verified the premise: neither `SBSP` nor `ROAD` had a `RecordType`
constant or any dispatch reference. The GRUP walker already tolerates
unhandled record types cleanly (0 walker errors over the full 63-distinct-
type `Oblivion.esm` walk the issue cites), so this was purely a coverage/
documentation gap, not a functional bug — matching the issue's own "Low"
impact framing and its first suggested option.

Applied that option: recorded both as deliberate non-goals rather than
adding parsers. Added `RecordType::SBSP` / `RecordType::ROAD` constants
(`crates/plugin/src/record.rs`, in the `WorldData` group alongside
`WRLD`/`CLMT`/`LSCR` — the closest existing category) with a doc comment
each explaining why no parser exists: SBSP is an Oblivion-only collision-
volume concept with no current physics consumer, ROAD only ever appears
twice in the whole game and matters only if worldspace road rendering is
ever wanted. Neither needs a `render_layer()` arm — both are not on the
REFR placement path, so they correctly fall through to the method's own
documented safe default (`RenderLayer::Architecture`) exactly like every
other non-spawn-site record type already does.

This closes the issue's literal complaint ("no code references anywhere")
without over-committing: a future consumer can still add a parser, and
the constants + doc comments are the "gap is now measured" artifact the
issue asked for.

## TESTS (issue's own checklist item — conditional: "if parsers are
added, a regression test pins the counts")

No parser was added, so the counts-against-`Oblivion.esm` test the issue
conditions on doesn't apply. Added a smaller but still real regression
instead: `record_type_sbsp_and_road_are_known_but_unmodeled` pins both
halves of the actual decision made here — the FourCC bytes round-trip
correctly (`as_str()`/`from_4cc()`), and `render_layer()` falls through to
the documented safe default rather than silently landing in some other
layer's bucket.

**Reintroduce-and-revert verification**: temporarily typo'd `SBSP`'s
FourCC bytes to `SBS_` — confirmed the new test failed with the exact
mismatch. Restored the fix and reran — all 21 tests in
`byroredux-plugin`'s `record::tests` pass again.

## Verification

- `cargo check -p byroredux-plugin --tests`: clean (the pre-existing
  unrelated `grup_walker.rs:469` `unused_mut` warning is present and out
  of scope, as in every prior session run).
- `cargo test -q -p byroredux-plugin record::`: 21 passing, 0 failing
  (+1 new).
- `cargo test -q --no-fail-fast` (full workspace): **7166 passing, 0
  failing**.
