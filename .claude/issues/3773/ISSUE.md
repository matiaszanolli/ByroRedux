# #3773 — UI-D4-2026-08-30-03: the FO4 catalog's Command/Request kind is partly heuristic-derived and partly evidence-derived, and the array records which is which nowhere

**Severity**: LOW · **Location**: `crates/ui/src/catalog.rs`, `docs/engine/ui.md`
**Source**: `docs/audits/AUDIT_UI_2026-08-30.md` (UI-D4-2026-08-30-03)

The 269-entry FO4 catalog mixes two provenances (138 F4CF-reconstructed entries with `kind`
read from source, 131 entries #2966's corpus sweep added with `kind` inferred from a
`Get*`/`Is*`/`Should*`/`Can*`/`get*` name-prefix heuristic) with no marker distinguishing them
— a maintainer had to re-derive which 131 before trusting any entry's `kind`.

## Fix implemented

- Derived the exact 131-entry list **from git history**, not guessing: diffed
  `FALLOUT4_BGS_CODE_OBJECT_METHODS`'s method names before/after commit `0a87ca54` (#2966's own
  commit). Result: 138 unchanged (0 removed), 131 added — matching the commit message's stated
  counts exactly, confirming the derivation.
- Added `ScaleformKindProvenance { Measured, HeuristicNamePrefix }` and a `provenance` field on
  `ScaleformHostMethod`. New `command_heuristic`/`request_heuristic` constructors (sibling to the
  existing `command`/`request`, which stay implicitly `Measured`) — used **only** for the exact
  131 derived entries, rewritten via a targeted script scoped to the FO4 array (all 131 matched,
  0 misses).
- `docs/engine/ui.md`'s provenance paragraph now states the split explicitly and points at the
  new field + its pinning test, instead of only being recoverable from the narrative prose.

Regression tests (issue's own TESTS checklist item):
`fallout4_catalog_provenance_split_matches_the_2966_sweep` pins the exact 138/131 split;
`skyrim_catalog_is_entirely_measured` confirms the marker doesn't spuriously appear on the
Skyrim array (whose `kind` is a measured protocol fact, not a guess, per its own doc).

**SIBLING** (issue's own checklist item): checked the AVM1/Skyrim half — confirmed via the new
test that it's uniformly `Measured` today (correct: #3103's own corpus sweep for Skyrim hasn't
happened yet, so there's no heuristic-derived Skyrim entry to mark). The `command_heuristic`/
`request_heuristic` constructors this issue adds are directly reusable the moment #3103 lands
its own sweep — not folded into this fix, which stays scoped to marking what already exists.

Full workspace: `cargo test --no-fail-fast` 7045 passing, 0 failing.
